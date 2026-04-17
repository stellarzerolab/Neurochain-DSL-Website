use std::{
    collections::{HashMap, VecDeque},
    env, fs,
    net::SocketAddr,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex as StdMutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
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
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{mpsc, Mutex, Semaphore},
    task,
    time::{sleep_until, timeout, Duration, Instant},
};
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    inference_sem: Arc<Semaphore>,
    repl_sem: Arc<Semaphore>,
    repl_acquire_timeout: Duration,
    repl_max_per_client: usize,
    repl_sessions_by_client: Arc<StdMutex<HashMap<String, usize>>>,
    allow_flow: bool,
    keygen_by_client: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    keygen_window: Duration,
    keygen_max_per_window: usize,
    session_idle_ttl: Duration,
}

#[derive(Deserialize, Debug, Default)]
struct StellarReplWsReq {
    #[serde(default)]
    debug: Option<String>,
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
static ALLOWED_ORIGINS: OnceLock<Vec<String>> = OnceLock::new();

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

fn normalize_origin(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

fn allowed_origins() -> &'static [String] {
    ALLOWED_ORIGINS.get_or_init(|| {
        let from_env = env::var("NC_STELLAR_DEMO_ALLOWED_ORIGINS")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .filter_map(normalize_origin)
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        if !from_env.is_empty() {
            return from_env;
        }

        vec![
            "https://stellarzerolab.art".to_string(),
            "https://www.stellarzerolab.art".to_string(),
        ]
    })
}

fn origin_allowed(headers: &HeaderMap) -> bool {
    let allowlist = allowed_origins();
    if allowlist.iter().any(|v| v == "*") {
        return true;
    }

    let provided = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .and_then(normalize_origin);

    // Browser WS includes Origin. For non-browser clients, missing origin is allowed.
    let Some(origin) = provided else {
        return true;
    };

    allowlist.iter().any(|allowed| allowed == &origin)
}

fn parse_bool_value(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
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

fn demo_allow_flow() -> bool {
    let primary = env::var("NC_DEMO_ALLOW_FLOW")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let fallback = env::var("NC_STELLAR_DEMO_ALLOW_FLOW")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(raw) = primary.as_deref().or(fallback.as_deref()) {
        return parse_bool_value(raw).unwrap_or(false);
    }
    false
}

fn default_repl_bin_path() -> String {
    if let Some(path) = env::var("NC_STELLAR_REPL_BIN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return path;
    }

    let fallback_name = if cfg!(windows) {
        "neurochain-stellar.exe"
    } else {
        "neurochain-stellar"
    };
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            let sibling = dir.join(fallback_name);
            if sibling.exists() {
                return sibling.to_string_lossy().to_string();
            }
        }
    }

    "neurochain-stellar".to_string()
}

fn ws_text_message(text: impl Into<String>) -> Message {
    Message::Text(text.into().into())
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(default)
}

fn extract_client_key(headers: &HeaderMap) -> String {
    if let Some(raw) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = raw
            .split(',')
            .map(str::trim)
            .find(|v| !v.is_empty())
            .map(str::to_string)
        {
            return first;
        }
    }

    if let Some(real_ip) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return real_ip.to_string();
    }

    "unknown-client".to_string()
}

fn strip_wrapping_quotes(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.as_bytes()[0] as char;
        let last = trimmed.as_bytes()[trimmed.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return trimmed[1..trimmed.len() - 1].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn parse_value_after_colon(input: &str) -> Option<String> {
    let (_, rhs) = input.split_once(':')?;
    let value = strip_wrapping_quotes(rhs);
    if value.is_empty() {
        return None;
    }
    Some(value)
}

fn parse_value_after_prefix<'a>(input: &'a str, prefix: &str) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    if !lower.starts_with(prefix) {
        return None;
    }
    let value = strip_wrapping_quotes(input[prefix.len()..].trim_start());
    if value.is_empty() {
        return None;
    }
    Some(value)
}

fn parse_keygen_alias_from_payload(payload: &str) -> Option<String> {
    for raw_line in payload.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("wallet_generate:") || lower.starts_with("wallet_bootstrap:") {
            if let Some(alias) = parse_value_after_colon(line) {
                return Some(alias);
            }
        }
        for prefix in [
            "wallet generate ",
            "wallet bootstrap ",
            "keys generate ",
            "generate wallet ",
            "wallet_generate ",
            "wallet_bootstrap ",
            "bootstrap wallet ",
        ] {
            if let Some(alias) = parse_value_after_prefix(line, prefix) {
                return Some(alias);
            }
        }
    }
    None
}

fn create_session_home() -> std::io::Result<PathBuf> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let pid = std::process::id();
    let root = env::temp_dir().join("neurochain-stellar-demo-sessions");
    std::fs::create_dir_all(&root)?;

    for attempt in 0u32..128 {
        let dir = root.join(format!("session-{now_ms}-{pid}-{attempt}"));
        match std::fs::create_dir(&dir) {
            Ok(()) => {
                std::fs::create_dir_all(dir.join(".config"))?;
                return Ok(dir);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "failed to allocate unique session directory",
    ))
}

async fn cleanup_session_home(path: Option<PathBuf>) {
    let Some(path) = path else {
        return;
    };
    let _ = tokio::fs::remove_dir_all(path).await;
}

async fn check_and_track_keygen_limit(state: &AppState, client_key: &str) -> Result<(), String> {
    let now = Instant::now();
    let mut guard = state.keygen_by_client.lock().await;
    let queue = guard
        .entry(client_key.to_string())
        .or_insert_with(VecDeque::new);

    while let Some(ts) = queue.front().copied() {
        if now.duration_since(ts) > state.keygen_window {
            queue.pop_front();
        } else {
            break;
        }
    }

    if queue.len() >= state.keygen_max_per_window {
        let mins = (state.keygen_window.as_secs() / 60).max(1);
        return Err(format!(
            "rate limit: wallet key generation is limited to {} per {} min per client",
            state.keygen_max_per_window, mins
        ));
    }

    queue.push_back(now);
    Ok(())
}

fn try_acquire_client_repl_slot(state: &AppState, client_key: &str) -> Result<usize, String> {
    let mut guard = state
        .repl_sessions_by_client
        .lock()
        .map_err(|_| "internal session counter lock error".to_string())?;
    let active = guard.entry(client_key.to_string()).or_insert(0);
    if *active >= state.repl_max_per_client {
        return Err(format!(
            "BUSY: per-client REPL session limit reached (max {} active sessions per client)",
            state.repl_max_per_client
        ));
    }
    *active += 1;
    Ok(*active)
}

fn release_client_repl_slot(state: &AppState, client_key: &str) {
    let Ok(mut guard) = state.repl_sessions_by_client.lock() else {
        return;
    };
    if let Some(active) = guard.get_mut(client_key) {
        if *active <= 1 {
            guard.remove(client_key);
        } else {
            *active -= 1;
        }
    }
}

struct ClientReplSessionGuard {
    state: Arc<AppState>,
    client_key: String,
}

impl ClientReplSessionGuard {
    fn new(state: Arc<AppState>, client_key: String) -> Self {
        Self { state, client_key }
    }
}

impl Drop for ClientReplSessionGuard {
    fn drop(&mut self) {
        release_client_repl_slot(&self.state, &self.client_key);
    }
}

fn append_output_scan_buffer(scan: &mut String, chunk: &str, max_len: usize) {
    scan.push_str(chunk);
    if scan.len() > max_len {
        let drain_len = scan.len() - max_len;
        scan.drain(..drain_len);
    }
}

fn keygen_success_seen(scan: &str, alias: &str) -> bool {
    scan.contains(&format!("Wallet key alias generated: {alias}"))
}

fn keygen_failure_seen(scan: &str, alias: &str) -> bool {
    scan.contains(&format!("wallet_generate failed for `{alias}`"))
        || scan.contains(&format!("wallet_bootstrap failed for `{alias}`"))
        || scan.contains(&format!("key generation failed for alias `{alias}`"))
        || scan.contains(&format!("failed to read address for alias `{alias}`"))
}

async fn stream_child_output<R>(mut reader: R, tx: mpsc::UnboundedSender<String>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                if tx.send(chunk).is_err() {
                    break;
                }
            }
            Err(err) => {
                let _ = tx.send(format!("\r\n[repl] stream read error: {err}\r\n"));
                break;
            }
        }
    }
}

async fn handle_stellar_repl_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    debug: bool,
    client_key: String,
) {
    let active_for_client = match try_acquire_client_repl_slot(&state, &client_key) {
        Ok(active) => active,
        Err(reason) => {
            let _ = socket
                .send(ws_text_message(format!("\r\n[repl] {reason}\r\n")))
                .await;
            return;
        }
    };
    let _client_slot_guard = ClientReplSessionGuard::new(state.clone(), client_key.clone());
    eprintln!(
        "repl session opened: client={} active_for_client={} per_client_max={}",
        client_key, active_for_client, state.repl_max_per_client
    );

    let _permit = match timeout(
        state.repl_acquire_timeout,
        state.repl_sem.clone().acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => permit,
        Ok(Err(_)) => {
            let _ = socket
                .send(ws_text_message(
                    "[repl] server is shutting down, cannot open REPL session\r\n",
                ))
                .await;
            return;
        }
        Err(_) => {
            let max = state.repl_sem.available_permits();
            let _ = socket
                .send(ws_text_message(
                    "[repl] BUSY: REPL sessions full; please retry shortly.\r\n",
                ))
                .await;
            eprintln!(
                "repl busy: session slot acquire timed out after {} ms (available_permits={})",
                state.repl_acquire_timeout.as_millis(),
                max
            );
            return;
        }
    };

    let repl_bin = default_repl_bin_path();
    let session_home = match create_session_home() {
        Ok(path) => path,
        Err(err) => {
            let _ = socket
                .send(ws_text_message(format!(
                    "[repl] failed to initialize session storage: {err}\r\n"
                )))
                .await;
            return;
        }
    };

    let mut cmd = Command::new(&repl_bin);
    cmd.arg("--repl");
    if !state.allow_flow {
        cmd.arg("--no-flow");
    }
    if debug {
        cmd.arg("--debug");
    }
    cmd.env_remove("NO_COLOR");
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.env("HOME", &session_home);
    cmd.env("XDG_CONFIG_HOME", session_home.join(".config"));
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            let _ = socket
                .send(ws_text_message(format!(
                    "[repl] failed to start `{repl_bin}`: {err}\r\n"
                )))
                .await;
            cleanup_session_home(Some(session_home)).await;
            return;
        }
    };

    let mut child_stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            let _ = socket
                .send(ws_text_message(
                    "[repl] failed: child stdin not available\r\n",
                ))
                .await;
            let _ = child.start_kill();
            let _ = child.wait().await;
            cleanup_session_home(Some(session_home)).await;
            return;
        }
    };
    let child_stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = socket
                .send(ws_text_message(
                    "[repl] failed: child stdout not available\r\n",
                ))
                .await;
            let _ = child.start_kill();
            let _ = child.wait().await;
            cleanup_session_home(Some(session_home)).await;
            return;
        }
    };
    let child_stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = socket
                .send(ws_text_message(
                    "[repl] failed: child stderr not available\r\n",
                ))
                .await;
            let _ = child.start_kill();
            let _ = child.wait().await;
            cleanup_session_home(Some(session_home)).await;
            return;
        }
    };

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(stream_child_output(child_stdout, out_tx.clone()));
    tokio::spawn(stream_child_output(child_stderr, out_tx));
    let mut last_activity = Instant::now();
    let mut active_wallet_alias: Option<String> = None;
    let mut pending_wallet_alias: Option<String> = None;
    let mut output_scan_buf = String::new();

    loop {
        let idle_deadline = last_activity + state.session_idle_ttl;
        tokio::select! {
            maybe_chunk = out_rx.recv() => {
                if let Some(chunk) = maybe_chunk {
                    last_activity = Instant::now();
                    append_output_scan_buffer(&mut output_scan_buf, &chunk, 16 * 1024);

                    let mut repl_notice: Option<String> = None;
                    if let Some(alias) = pending_wallet_alias.as_deref() {
                        if keygen_success_seen(&output_scan_buf, alias) {
                            let alias = alias.to_string();
                            active_wallet_alias = Some(alias.clone());
                            pending_wallet_alias = None;
                            repl_notice = Some(format!(
                                "\r\n[repl] wallet alias locked for this session: `{alias}`\r\n"
                            ));
                        } else if keygen_failure_seen(&output_scan_buf, alias) {
                            pending_wallet_alias = None;
                            repl_notice = Some(
                                "\r\n[repl] wallet key generation failed; alias not locked for this session\r\n".to_string(),
                            );
                        }
                    }

                    if socket.send(ws_text_message(chunk)).await.is_err() {
                        break;
                    }
                    if let Some(notice) = repl_notice {
                        if socket.send(ws_text_message(notice)).await.is_err() {
                            break;
                        }
                    }
                }
            }
            _ = sleep_until(idle_deadline) => {
                let _ = socket
                    .send(ws_text_message(format!(
                        "\r\n[repl] session expired after {} sec idle\r\n",
                        state.session_idle_ttl.as_secs()
                    )))
                    .await;
                break;
            }
            child_status = child.wait() => {
                match child_status {
                    Ok(status) => {
                        let _ = socket
                            .send(ws_text_message(format!("\r\n[repl] process exited ({status})\r\n")))
                            .await;
                    }
                    Err(err) => {
                        let _ = socket
                            .send(ws_text_message(format!("\r\n[repl] process wait error: {err}\r\n")))
                            .await;
                    }
                }
                break;
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        last_activity = Instant::now();
                        let payload = text.to_string();
                        if payload.is_empty() {
                            continue;
                        }
                        if let Some(alias) = parse_keygen_alias_from_payload(&payload) {
                            let normalized_alias = alias.trim().to_string();
                            if normalized_alias.is_empty() {
                                let _ = socket
                                    .send(ws_text_message("\r\n[repl] wallet alias is empty\r\n"))
                                    .await;
                                continue;
                            }

                            if let Some(existing) = active_wallet_alias.as_deref() {
                                if existing != normalized_alias {
                                    let _ = socket
                                        .send(ws_text_message(format!(
                                            "\r\n[repl] session already has active wallet alias `{existing}`; reset session before creating another keypair\r\n"
                                        )))
                                        .await;
                                    continue;
                                }
                            } else {
                                if let Some(pending) = pending_wallet_alias.as_deref() {
                                    if pending != normalized_alias {
                                        let _ = socket
                                            .send(ws_text_message(format!(
                                                "\r\n[repl] wallet key generation already in progress for alias `{pending}`; wait for result or reset session\r\n"
                                            )))
                                            .await;
                                        continue;
                                    }
                                } else if let Err(reason) =
                                    check_and_track_keygen_limit(&state, &client_key).await
                                {
                                    let _ = socket
                                        .send(ws_text_message(format!("\r\n[repl] {reason}\r\n")))
                                        .await;
                                    continue;
                                } else {
                                    pending_wallet_alias = Some(normalized_alias.clone());
                                    let _ = socket
                                        .send(ws_text_message(format!(
                                            "\r\n[repl] wallet key generation in progress for alias `{normalized_alias}`\r\n"
                                        )))
                                        .await;
                                }
                            }
                        }
                        if child_stdin.write_all(payload.as_bytes()).await.is_err() {
                            break;
                        }
                        let _ = child_stdin.flush().await;
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        last_activity = Instant::now();
                        if child_stdin.write_all(&bytes).await.is_err() {
                            break;
                        }
                        let _ = child_stdin.flush().await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        let _ = socket
                            .send(ws_text_message(format!("\r\n[repl] websocket error: {err}\r\n")))
                            .await;
                        break;
                    }
                }
            }
        }
    }

    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(1), child.wait()).await;
    cleanup_session_home(Some(session_home)).await;
}

async fn api_stellar_repl_ws(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(req): Query<StellarReplWsReq>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !origin_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }

    if let Some(required) = required_api_key() {
        let ok = provided_api_key(&headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    let debug = req
        .debug
        .as_deref()
        .and_then(parse_bool_value)
        .unwrap_or(false);
    let client_key = extract_client_key(&headers);
    ws.on_upgrade(move |socket| handle_stellar_repl_socket(socket, state, debug, client_key))
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
                );
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

#[tokio::main]
async fn main() {
    banner::print_banner();

    if required_api_key().is_none() {
        eprintln!(
            "NC_API_KEY is required for neurochain-stellar-demo-server (refusing to start without auth)"
        );
        std::process::exit(2);
    }

    let host = env::var("NC_STELLAR_DEMO_HOST")
        .or_else(|_| env::var("HOST"))
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("NC_STELLAR_DEMO_PORT")
        .ok()
        .or_else(|| env::var("PORT").ok())
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8082);
    let max_infer: usize = env::var("NC_MAX_INFER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let max_repl_sessions: usize = env::var("NC_MAX_REPL_SESSIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let max_repl_sessions_per_client = env_usize("NC_MAX_REPL_SESSIONS_PER_CLIENT", 2).max(1);
    let keygen_window_secs = env_u64("NC_STELLAR_KEYGEN_WINDOW_SECS", 600);
    let keygen_max_per_window = env_usize("NC_STELLAR_KEYGEN_MAX_PER_WINDOW", 3);
    let session_idle_ttl_secs = env_u64("NC_STELLAR_SESSION_IDLE_TTL_SECS", 300);
    let repl_acquire_timeout_ms = env_u64("NC_REPL_ACQUIRE_TIMEOUT_MS", 2500);

    let state = Arc::new(AppState {
        inference_sem: Arc::new(Semaphore::new(max_infer)),
        repl_sem: Arc::new(Semaphore::new(max_repl_sessions)),
        repl_acquire_timeout: Duration::from_millis(repl_acquire_timeout_ms.max(250)),
        repl_max_per_client: max_repl_sessions_per_client,
        repl_sessions_by_client: Arc::new(StdMutex::new(HashMap::new())),
        allow_flow: demo_allow_flow(),
        keygen_by_client: Arc::new(Mutex::new(HashMap::new())),
        keygen_window: Duration::from_secs(keygen_window_secs.max(60)),
        keygen_max_per_window: keygen_max_per_window.max(1),
        session_idle_ttl: Duration::from_secs(session_idle_ttl_secs.max(60)),
    });
    let allow_flow = state.allow_flow;

    let api = Router::new()
        .route("/analyze", post(api_analyze))
        .route("/stellar/intent-plan", post(api_stellar_intent_plan))
        .route("/stellar/repl/ws", get(api_stellar_repl_ws))
        .with_state(state.clone());

    let app = Router::new().nest("/api", api).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid demo server bind address");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind demo server listener");

    println!(
        "NeuroChain Stellar demo server listening on http://{addr} (demo_flow={})",
        if allow_flow { "on" } else { "off" }
    );
    println!("Allowed WS origins: {}", allowed_origins().join(", "));
    println!(
        "Session guardrails: keygen={} per {}s per client, idle_ttl={}s, repl_acquire_timeout={}ms, repl_per_client_max={}",
        state.keygen_max_per_window,
        state.keygen_window.as_secs(),
        state.session_idle_ttl.as_secs(),
        state.repl_acquire_timeout.as_millis(),
        state.repl_max_per_client
    );

    axum::serve(listener, app).await.expect("serve demo server");
}
