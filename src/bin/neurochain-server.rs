use std::{
    collections::HashMap,
    env, fs,
    net::SocketAddr,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, OnceLock},
};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use neurochain::{
    actions::{validate_plan, Action, ActionPlan, Allowlist},
    banner, engine,
    intent_stellar::{
        build_action_plan as build_intent_action_plan, classify as classify_intent_stellar,
        has_intent_blocking_issue, resolve_model_path as resolve_intent_model_path,
        threshold_from_env as intent_threshold_from_env, DEFAULT_INTENT_STELLAR_THRESHOLD,
    },
    interpreter,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    sync::Semaphore,
    task,
    time::{timeout, Duration},
};
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    inference_sem: Arc<Semaphore>,
}

#[derive(Deserialize, Debug)]
struct AnalyzeReq {
    #[serde(default)]
    model: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Serialize)]
struct AnalyzeResp {
    ok: bool,
    output: String,
    logs: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct StellarIntentPlanReq {
    prompt: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    model_path: Option<String>,
    #[serde(default)]
    threshold: Option<f32>,
    #[serde(default)]
    allowlist_assets: Option<String>,
    #[serde(default)]
    allowlist_contracts: Option<String>,
    #[serde(default)]
    allowlist_enforce: Option<bool>,
    #[serde(default)]
    contract_policy_enforce: Option<bool>,
}

#[derive(Serialize)]
struct StellarIntentPlanResp {
    ok: bool,
    blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    plan: ActionPlan,
    logs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ArgSchema {
    #[serde(default)]
    required: HashMap<String, String>,
    #[serde(default)]
    optional: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ContractPolicy {
    contract_id: String,
    #[serde(default)]
    allowed_functions: Vec<String>,
    #[serde(default)]
    args_schema: HashMap<String, ArgSchema>,
    #[serde(default)]
    max_fee_stroops: Option<u64>,
    #[serde(default)]
    resource_limits: Option<Value>,
}

static REQUIRED_API_KEY: OnceLock<Option<String>> = OnceLock::new();

fn required_api_key() -> Option<&'static str> {
    REQUIRED_API_KEY
        .get_or_init(|| {
            env::var("NC_API_KEY")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .as_deref()
}

fn provided_api_key(headers: &HeaderMap) -> Option<&str> {
    let from_x_api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if from_x_api_key.is_some() {
        return from_x_api_key;
    }

    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;

    const PREFIX: &str = "Bearer ";
    if auth.len() > PREFIX.len() && auth[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        return Some(auth[PREFIX.len()..].trim());
    }

    None
}

fn secure_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn models_base() -> String {
    env::var("NC_MODELS_DIR").unwrap_or_else(|_| "/opt/neurochain/models".to_string())
}

fn resolve_model_path(id: &str) -> Option<String> {
    let base = models_base();
    let path = match id {
        "sst2" => format!("{base}/distilbert-sst2/model.onnx"),
        "factcheck" => format!("{base}/factcheck/model.onnx"),
        "intent" => format!("{base}/intent/model.onnx"),
        "intent_stellar" | "stellar_intent" => format!("{base}/intent_stellar/model.onnx"),
        "toxic" => format!("{base}/toxic_quantized/model.onnx"),
        "macro" | "intent_macro" | "macro_intent" | "gpt2" | "generator" => {
            format!("{base}/intent_macro/model.onnx")
        }
        _ => return None,
    };
    Some(path)
}

fn parse_bool_value(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn allowlist_enforced(override_value: Option<bool>) -> bool {
    if let Some(value) = override_value {
        return value;
    }
    parse_bool_value(&env::var("NC_ALLOWLIST_ENFORCE").unwrap_or_default()).unwrap_or(false)
}

fn policy_enforced(override_value: Option<bool>) -> bool {
    if let Some(value) = override_value {
        return value;
    }
    matches!(
        env::var("NC_CONTRACT_POLICY_ENFORCE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn load_contract_policies() -> Vec<ContractPolicy> {
    let mut policies = Vec::new();

    if let Ok(path) = env::var("NC_CONTRACT_POLICY") {
        if let Ok(data) = fs::read_to_string(&path) {
            match serde_json::from_str::<ContractPolicy>(&data) {
                Ok(policy) => policies.push(policy),
                Err(err) => eprintln!("Policy parse failed for {path}: {err}"),
            }
        } else {
            eprintln!("Policy file not found: {path}");
        }
    }

    let policy_dir = env::var("NC_CONTRACT_POLICY_DIR").unwrap_or_else(|_| "contracts".to_string());
    if let Ok(entries) = fs::read_dir(&policy_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let policy_path = path.join("policy.json");
                if let Ok(data) = fs::read_to_string(&policy_path) {
                    match serde_json::from_str::<ContractPolicy>(&data) {
                        Ok(policy) => policies.push(policy),
                        Err(err) => {
                            eprintln!("Policy parse failed for {}: {err}", policy_path.display())
                        }
                    }
                }
            }
        }
    }

    policies
}

fn is_base32_char(c: char) -> bool {
    matches!(c, 'A'..='Z' | '2'..='7')
}

fn is_strkey(value: &str) -> bool {
    if value.len() != 56 {
        return false;
    }
    let first = value.chars().next().unwrap_or('\0');
    if first != 'G' && first != 'C' {
        return false;
    }
    value.chars().all(is_base32_char)
}

fn is_symbol(value: &str) -> bool {
    let len = value.len();
    if len == 0 || len > 32 {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii() && !c.is_control() && !c.is_whitespace())
}

fn is_hex_bytes(value: &str) -> bool {
    if !value.starts_with("0x") {
        return false;
    }
    let hex = &value[2..];
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return false;
    }
    hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_u64_value(value: &Value) -> bool {
    if value.as_u64().is_some() {
        return true;
    }
    value
        .as_str()
        .map(|s| s.trim().parse::<u64>().is_ok())
        .unwrap_or(false)
}

fn typed_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            if n.is_i64() {
                "i64"
            } else if n.is_u64() {
                "u64"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn typed_value_preview(value: &Value) -> String {
    let mut rendered = value.to_string();
    if rendered.len() > 96 {
        rendered.truncate(93);
        rendered.push_str("...");
    }
    rendered
}

fn validate_arg_type(value: &Value, ty: &str) -> bool {
    match ty {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "bool" => value.is_boolean(),
        "address" => value.as_str().map(is_strkey).unwrap_or(false),
        "symbol" => value.as_str().map(is_symbol).unwrap_or(false),
        "bytes" => value.as_str().map(is_hex_bytes).unwrap_or(false),
        "u64" => is_u64_value(value),
        _ => false,
    }
}

fn is_typed_template_v2_type(ty: &str) -> bool {
    matches!(ty, "address" | "bytes" | "symbol" | "u64")
}

fn normalize_typed_slot_value(value: &mut Value, ty: &str) -> Result<bool, String> {
    match ty {
        "address" => {
            let Some(raw) = value.as_str() else {
                return Err(format!(
                    "expected address got {} value={}",
                    typed_value_kind(value),
                    typed_value_preview(value)
                ));
            };
            let normalized = raw.trim().to_ascii_uppercase();
            if !is_strkey(&normalized) {
                return Err(format!(
                    "expected address got string value={}",
                    typed_value_preview(&Value::String(raw.to_string()))
                ));
            }
            let changed = normalized != raw;
            if changed {
                *value = Value::String(normalized);
            }
            Ok(changed)
        }
        "bytes" => {
            let Some(raw) = value.as_str() else {
                return Err(format!(
                    "expected bytes got {} value={}",
                    typed_value_kind(value),
                    typed_value_preview(value)
                ));
            };
            let trimmed = raw.trim();
            let (had_prefix, body) = if let Some(rest) = trimmed.strip_prefix("0x") {
                (true, rest)
            } else if let Some(rest) = trimmed.strip_prefix("0X") {
                (true, rest)
            } else {
                (false, trimmed)
            };
            let compact: String = body
                .chars()
                .filter(|c| !(c.is_ascii_whitespace() || matches!(c, '_' | '-')))
                .collect();
            let mut normalized = if had_prefix {
                format!("0x{compact}")
            } else {
                compact.clone()
            };
            if !had_prefix
                && !compact.is_empty()
                && compact.len().is_multiple_of(2)
                && compact.chars().all(|c| c.is_ascii_hexdigit())
            {
                normalized = format!("0x{compact}");
            }
            if normalized.starts_with("0x") {
                let lower_hex = normalized[2..].to_ascii_lowercase();
                normalized = format!("0x{lower_hex}");
            }
            if !is_hex_bytes(&normalized) {
                return Err(format!(
                    "expected bytes got string value={}",
                    typed_value_preview(&Value::String(raw.to_string()))
                ));
            }
            let changed = normalized != raw;
            if changed {
                *value = Value::String(normalized);
            }
            Ok(changed)
        }
        "symbol" => {
            let Some(raw) = value.as_str() else {
                return Err(format!(
                    "expected symbol got {} value={}",
                    typed_value_kind(value),
                    typed_value_preview(value)
                ));
            };
            let normalized = raw.trim().to_string();
            if !is_symbol(&normalized) {
                return Err(format!(
                    "expected symbol got string value={}",
                    typed_value_preview(&Value::String(raw.to_string()))
                ));
            }
            let changed = normalized != raw;
            if changed {
                *value = Value::String(normalized);
            }
            Ok(changed)
        }
        "u64" => {
            if value.as_u64().is_some() {
                return Ok(false);
            }
            if let Some(raw) = value.as_str() {
                let trimmed = raw.trim();
                let compact: String = trimmed
                    .chars()
                    .filter(|c| !matches!(c, '_' | ','))
                    .collect();
                let parsed = compact.parse::<u64>().map_err(|_| {
                    format!(
                        "expected u64 got string value={}",
                        typed_value_preview(&Value::String(raw.to_string()))
                    )
                })?;
                let new_value = Value::Number(parsed.into());
                let changed = *value != new_value;
                *value = new_value;
                return Ok(changed);
            }
            Err(format!(
                "expected u64 got {} value={}",
                typed_value_kind(value),
                typed_value_preview(value)
            ))
        }
        _ => Ok(false),
    }
}

#[derive(Default)]
struct PolicyTypedV2Outcome {
    errors: Vec<String>,
    normalized_args: usize,
}

fn apply_policy_typed_schema_to_args(
    contract_id: &str,
    function: &str,
    args: &mut Value,
    schema: &ArgSchema,
) -> PolicyTypedV2Outcome {
    let Some(args_obj) = args.as_object() else {
        return PolicyTypedV2Outcome::default();
    };
    let mut outcome = PolicyTypedV2Outcome::default();
    let mut updates: Vec<(String, Value)> = Vec::new();

    for (key, ty_raw) in &schema.required {
        let ty = ty_raw.trim().to_ascii_lowercase();
        if !is_typed_template_v2_type(ty.as_str()) {
            continue;
        }
        if let Some(value) = args_obj.get(key) {
            let mut normalized = value.clone();
            match normalize_typed_slot_value(&mut normalized, ty.as_str()) {
                Ok(changed) => {
                    if !validate_arg_type(&normalized, ty.as_str()) {
                        outcome.errors.push(format!(
                            "slot_type_error: ContractInvoke {key} expected {ty} (policy {contract_id}:{function})"
                        ));
                        continue;
                    }
                    if changed && &normalized != value {
                        updates.push((key.clone(), normalized));
                    }
                }
                Err(detail) => outcome.errors.push(format!(
                    "slot_type_error: ContractInvoke {key} {detail} (policy {contract_id}:{function})"
                )),
            }
        }
    }

    for (key, ty_raw) in &schema.optional {
        let ty = ty_raw.trim().to_ascii_lowercase();
        if !is_typed_template_v2_type(ty.as_str()) {
            continue;
        }
        if let Some(value) = args_obj.get(key) {
            let mut normalized = value.clone();
            match normalize_typed_slot_value(&mut normalized, ty.as_str()) {
                Ok(changed) => {
                    if !validate_arg_type(&normalized, ty.as_str()) {
                        outcome.errors.push(format!(
                            "slot_type_error: ContractInvoke {key} expected {ty} (policy {contract_id}:{function})"
                        ));
                        continue;
                    }
                    if changed && &normalized != value {
                        updates.push((key.clone(), normalized));
                    }
                }
                Err(detail) => outcome.errors.push(format!(
                    "slot_type_error: ContractInvoke {key} {detail} (policy {contract_id}:{function})"
                )),
            }
        }
    }

    if let Some(args_obj_mut) = args.as_object_mut() {
        for (key, value) in updates {
            args_obj_mut.insert(key, value);
            outcome.normalized_args += 1;
        }
    }

    outcome
}

fn apply_policy_typed_templates_v2(
    plan: &mut ActionPlan,
    policies: &[ContractPolicy],
) -> (usize, usize) {
    if policies.is_empty() {
        return (0, 0);
    }

    let mut policy_map: HashMap<&str, &ContractPolicy> = HashMap::new();
    for policy in policies {
        policy_map.insert(policy.contract_id.as_str(), policy);
    }

    let mut converted = 0usize;
    let mut normalized_args = 0usize;
    for action in &mut plan.actions {
        let outcome = match action {
            Action::SorobanContractInvoke {
                contract_id,
                function,
                args,
            } => {
                let Some(policy) = policy_map.get(contract_id.as_str()) else {
                    continue;
                };
                let Some(schema) = policy.args_schema.get(function) else {
                    continue;
                };

                apply_policy_typed_schema_to_args(contract_id, function, args, schema)
            }
            _ => PolicyTypedV2Outcome::default(),
        };
        normalized_args += outcome.normalized_args;

        if let Some(reason) = outcome.errors.first().cloned() {
            *action = Action::Unknown {
                reason: reason.clone(),
            };
            for err in outcome.errors {
                plan.warnings.push(format!("intent_error: {err}"));
            }
            converted += 1;
        }
    }

    (converted, normalized_args)
}

fn validate_contract_policies(
    plan: &ActionPlan,
    policies: &[ContractPolicy],
) -> (Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    if policies.is_empty() {
        return (warnings, errors);
    }

    let mut map: HashMap<String, ContractPolicy> = HashMap::new();
    for policy in policies {
        map.insert(policy.contract_id.clone(), policy.clone());
    }

    for action in &plan.actions {
        if let Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } = action
        {
            let Some(policy) = map.get(contract_id) else {
                errors.push(format!(
                    "policy_missing: no policy for contract_id {contract_id}"
                ));
                continue;
            };
            if !policy.allowed_functions.is_empty()
                && !policy.allowed_functions.iter().any(|f| f == function)
            {
                errors.push(format!(
                    "policy_function_denied: {contract_id}:{function} not allowed"
                ));
                continue;
            }

            if let Some(schema) = policy.args_schema.get(function) {
                let args_obj = args.as_object();
                if args_obj.is_none() {
                    errors.push(format!(
                        "policy_args_invalid: {contract_id}:{function} args must be object"
                    ));
                    continue;
                }
                let args_obj = args_obj.expect("checked is_some above");

                for (key, ty) in &schema.required {
                    match args_obj.get(key) {
                        Some(val) => {
                            if !validate_arg_type(val, ty) {
                                errors.push(format!(
                                    "policy_args_type: {contract_id}:{function} {key} expected {ty}"
                                ));
                            }
                        }
                        None => errors.push(format!(
                            "policy_args_missing: {contract_id}:{function} missing {key}"
                        )),
                    }
                }

                for (key, ty) in &schema.optional {
                    if let Some(val) = args_obj.get(key) {
                        if !validate_arg_type(val, ty) {
                            errors.push(format!(
                                "policy_args_type: {contract_id}:{function} {key} expected {ty}"
                            ));
                        }
                    }
                }

                for key in args_obj.keys() {
                    if !schema.required.contains_key(key) && !schema.optional.contains_key(key) {
                        warnings.push(format!(
                            "policy_args_unknown: {contract_id}:{function} unexpected arg {key}"
                        ));
                    }
                }
            }

            if let Some(limits) = &policy.resource_limits {
                if !limits.is_object() {
                    warnings.push(format!(
                        "policy_resource_limits_invalid: {contract_id} resource_limits must be object"
                    ));
                }
            }

            if let Some(max_fee) = policy.max_fee_stroops {
                warnings.push(format!(
                    "policy_hint: {contract_id}:{function} max_fee_stroops={max_fee}"
                ));
            }
        }
    }

    (warnings, errors)
}

fn normalize(s: &str) -> String {
    s.replace('\u{FEFF}', "")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ")
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::main]
async fn main() {
    banner::print_banner();
    std::panic::set_hook(Box::new(|info| {
        eprintln!("PANIC: {info}");
        if std::env::var("RUST_BACKTRACE").as_deref() != Ok("0") {
            eprintln!("(enable RUST_BACKTRACE=1 for backtrace)");
        }
    }));

    let max_infer: usize = env::var("NC_MAX_INFER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);

    let state = Arc::new(AppState {
        inference_sem: Arc::new(Semaphore::new(max_infer)),
    });

    let api = Router::new()
        .route("/analyze", post(api_analyze))
        .route("/stellar/intent-plan", post(api_stellar_intent_plan))
        .with_state(state);

    let app = Router::new().nest("/api", api).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );

    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8081);
    let addr: SocketAddr = format!("{host}:{port}").parse().expect("Invalid HOST/PORT");

    println!("NeuroChain API listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("ERROR: failed to bind to {addr}: {e}");
            eprintln!("Hint: is the port already in use?");
            eprintln!("  Linux:   `ss -tulpn | grep :{port}`");
            eprintln!("  Windows: `netstat -ano | findstr :{port}`");
            std::process::exit(1);
        });

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("ERROR: server error: {e}");
        std::process::exit(1);
    }
}

async fn api_analyze(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AnalyzeReq>,
) -> impl IntoResponse {
    let mut logs: Vec<String> = Vec::new();
    if !req.model.is_empty() {
        logs.push(format!("model={}", req.model));
    }

    if let Some(required) = required_api_key() {
        let ok = provided_api_key(&headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            logs.push("auth: missing or invalid api key".into());
            return (
                StatusCode::UNAUTHORIZED,
                Json(AnalyzeResp {
                    ok: false,
                    output: "ERROR: unauthorized".into(),
                    logs,
                }),
            );
        }
    }

    let mut code = req.code.or(req.content).unwrap_or_default();
    if code.trim().is_empty() {
        logs.push("warn: empty input".into());
        return (
            StatusCode::OK,
            Json(AnalyzeResp {
                ok: false,
                output: "ERROR: empty input".into(),
                logs,
            }),
        );
    }

    if let Some(path) = resolve_model_path(&req.model) {
        let has_ai = code.lines().any(|l| l.trim_start().starts_with("AI:"));
        if !has_ai {
            code = format!("AI: \"{path}\"\n{code}");
            logs.push(format!("auto: injected AI model path {}", path));
        }
    } else if !req.model.is_empty() {
        logs.push(format!("warn: unknown model id '{}'", req.model));
    }

    let code = normalize(&code);

    let permit = match state.inference_sem.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            let maybe = timeout(
                Duration::from_millis(50),
                state.inference_sem.clone().acquire_owned(),
            )
            .await;
            match maybe {
                Ok(Ok(p)) => p,
                _ => {
                    logs.push("busy: inference slots full".into());
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(AnalyzeResp {
                            ok: false,
                            output: "BUSY: inference slots full; please retry shortly.".into(),
                            logs,
                        }),
                    );
                }
            }
        }
    };

    let task_res = task::spawn_blocking(move || {
        catch_unwind(AssertUnwindSafe(|| {
            let mut interpreter = interpreter::Interpreter::new();
            engine::analyze(&code, &mut interpreter)
        }))
    })
    .await;

    drop(permit);

    let res = match task_res {
        Ok(inner) => inner,
        Err(e) => {
            logs.push(format!("join error: {e}"));
            return (
                StatusCode::OK,
                Json(AnalyzeResp {
                    ok: false,
                    output: "ERROR: internal join error in analyze()".into(),
                    logs,
                }),
            );
        }
    };

    match res {
        Ok(Ok(out)) => (
            StatusCode::OK,
            Json(AnalyzeResp {
                ok: true,
                output: out,
                logs,
            }),
        ),
        Ok(Err(e)) => (
            StatusCode::OK,
            Json(AnalyzeResp {
                ok: false,
                output: format!("ERROR: {e}"),
                logs,
            }),
        ),
        Err(panic) => {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "internal panic in analyze()".to_string()
            };
            (
                StatusCode::OK,
                Json(AnalyzeResp {
                    ok: false,
                    output: format!("ERROR: {msg}"),
                    logs,
                }),
            )
        }
    }
}

async fn api_stellar_intent_plan(
    _state: State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<StellarIntentPlanReq>,
) -> impl IntoResponse {
    let mut logs: Vec<String> = Vec::new();

    if let Some(required) = required_api_key() {
        let ok = provided_api_key(&headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            logs.push("auth: missing or invalid api key".into());
            return (
                StatusCode::UNAUTHORIZED,
                Json(StellarIntentPlanResp {
                    ok: false,
                    blocked: true,
                    exit_code: Some(1),
                    error: Some("unauthorized".to_string()),
                    plan: ActionPlan::default(),
                    logs,
                }),
            );
        }
    }

    let prompt = req.prompt.trim().to_string();
    if prompt.is_empty() {
        logs.push("warn: empty prompt".into());
        return (
            StatusCode::OK,
            Json(StellarIntentPlanResp {
                ok: false,
                blocked: true,
                exit_code: Some(2),
                error: Some("empty prompt".to_string()),
                plan: ActionPlan::default(),
                logs,
            }),
        );
    }

    let model_path = req
        .model_path
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| {
            req.model
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .and_then(resolve_model_path)
        })
        .unwrap_or_else(resolve_intent_model_path);
    logs.push(format!("model_path={model_path}"));

    let threshold = match req.threshold {
        Some(v) => v,
        None => match intent_threshold_from_env() {
            Ok(Some(v)) => v,
            Ok(None) => DEFAULT_INTENT_STELLAR_THRESHOLD,
            Err(err) => {
                return (
                    StatusCode::OK,
                    Json(StellarIntentPlanResp {
                        ok: false,
                        blocked: true,
                        exit_code: Some(2),
                        error: Some(format!("invalid intent threshold env: {err}")),
                        plan: ActionPlan::default(),
                        logs,
                    }),
                )
            }
        },
    };
    logs.push(format!("threshold={threshold:.2}"));

    let decision = match classify_intent_stellar(&prompt, &model_path, threshold) {
        Ok(decision) => decision,
        Err(err) => {
            return (
                StatusCode::OK,
                Json(StellarIntentPlanResp {
                    ok: false,
                    blocked: true,
                    exit_code: Some(1),
                    error: Some(format!("{err:#}")),
                    plan: ActionPlan::default(),
                    logs,
                }),
            );
        }
    };

    let mut plan = build_intent_action_plan(&prompt, &decision);
    plan.warnings
        .push(format!("intent_model: path={model_path}"));

    let policies = load_contract_policies();
    let (typed_v2_converted, typed_v2_normalized_args) =
        apply_policy_typed_templates_v2(&mut plan, &policies);
    logs.push(format!(
        "typed_template_v2: policy_slot_type_converted={typed_v2_converted} normalized_args={typed_v2_normalized_args}"
    ));

    let assets_raw = req
        .allowlist_assets
        .clone()
        .unwrap_or_else(|| env::var("NC_ASSET_ALLOWLIST").unwrap_or_default());
    let contracts_raw = req
        .allowlist_contracts
        .clone()
        .unwrap_or_else(|| env::var("NC_SOROBAN_ALLOWLIST").unwrap_or_default());
    let allowlist = Allowlist::from_raw(&assets_raw, &contracts_raw);
    let allowlist_violations = validate_plan(&plan, &allowlist);
    let allowlist_is_enforced = allowlist_enforced(req.allowlist_enforce);
    logs.push(format!(
        "allowlist: violations={} enforced={allowlist_is_enforced}",
        allowlist_violations.len()
    ));

    for violation in &allowlist_violations {
        plan.warnings.push(format!(
            "allowlist warning: #{} {} ({})",
            violation.index, violation.action, violation.reason
        ));
    }

    let (policy_warnings, policy_errors) = validate_contract_policies(&plan, &policies);
    let policy_is_enforced = policy_enforced(req.contract_policy_enforce);
    logs.push(format!(
        "policy: warnings={} errors={} enforced={policy_is_enforced}",
        policy_warnings.len(),
        policy_errors.len()
    ));

    for warning in &policy_warnings {
        plan.warnings.push(format!("policy warning: {warning}"));
    }
    for err in &policy_errors {
        plan.warnings.push(format!("policy error: {err}"));
    }

    let mut blocked = false;
    let mut exit_code = None;

    if allowlist_is_enforced && !allowlist_violations.is_empty() {
        blocked = true;
        exit_code = Some(3);
        logs.push("block: allowlist_enforced".to_string());
    }
    if policy_is_enforced && !policy_errors.is_empty() {
        blocked = true;
        if exit_code.is_none() {
            exit_code = Some(4);
        }
        logs.push("block: contract_policy_enforced".to_string());
    }
    if exit_code.is_none() && has_intent_blocking_issue(&plan) {
        blocked = true;
        exit_code = Some(5);
        logs.push("block: intent_safety".to_string());
    }

    (
        StatusCode::OK,
        Json(StellarIntentPlanResp {
            ok: !blocked,
            blocked,
            exit_code,
            error: None,
            plan,
            logs,
        }),
    )
}
