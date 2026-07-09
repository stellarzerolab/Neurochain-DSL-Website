use std::{
    env, fs,
    net::SocketAddr,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex, OnceLock},
};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use neurochain::{
    actions::{validate_enforced_plan, validate_plan, Action, ActionPlan, Allowlist},
    banner, engine,
    intent_stellar::{
        build_action_plan as build_intent_action_plan, classify as classify_intent_stellar,
        has_intent_blocking_issue, resolve_model_path as resolve_intent_model_path,
        threshold_from_env as intent_threshold_from_env, DEFAULT_INTENT_STELLAR_THRESHOLD,
    },
    interpreter, soroban_deep,
    soroban_deep::ContractPolicy,
    x402_facilitator::{build_x402_payment_verifier, X402PaymentVerification, X402PaymentVerifier},
    x402_stellar::{
        x402_error_response, x402_payment_required_response, x402_payment_signature,
        x402_stellar_decision_response, X402PaymentContext, X402StellarIntentPlanOutcome,
    },
    x402_store::{build_x402_challenge_store, now_unix_secs, X402ChallengeStore},
    zk_attestation::{inspect_zk_attestation, ZkAttestationViewRequest, ZkAttestationViewResponse},
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::Semaphore,
    task,
    time::{timeout, Duration},
};
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct AppState {
    inference_sem: Arc<Semaphore>,
    x402_stellar: Arc<Mutex<Box<dyn X402ChallengeStore + Send>>>,
    x402_payment_verifier: Arc<dyn X402PaymentVerifier + Send + Sync>,
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
    #[serde(default)]
    requires_approval: Option<bool>,
}

#[derive(Serialize)]
struct StellarIntentPlanResp {
    ok: bool,
    blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    requires_approval: bool,
    plan: ActionPlan,
    logs: Vec<String>,
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

fn is_false(value: &bool) -> bool {
    !*value
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

fn resolve_stellar_intent_model_path(
    req: &StellarIntentPlanReq,
    logs: &mut Vec<String>,
) -> Result<String, String> {
    if req
        .model_path
        .as_deref()
        .map(str::trim)
        .is_some_and(|v| !v.is_empty())
    {
        logs.push("warn: client model_path rejected".to_string());
        return Err("model_path is not accepted by the server API; use model id".to_string());
    }

    let model_path = req
        .model
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .and_then(resolve_model_path)
        .unwrap_or_else(resolve_intent_model_path);

    Ok(model_path)
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

#[derive(Debug, Default)]
struct ContractPolicyLoad {
    policies: Vec<ContractPolicy>,
    errors: Vec<String>,
}

fn load_contract_policies() -> ContractPolicyLoad {
    let mut policies = Vec::new();
    let mut errors = Vec::new();

    if let Ok(path) = env::var("NC_CONTRACT_POLICY") {
        if !path.trim().is_empty() {
            match fs::read_to_string(&path) {
                Ok(data) => match serde_json::from_str::<ContractPolicy>(&data) {
                    Ok(policy) => policies.push(policy),
                    Err(err) => {
                        let msg =
                            format!("policy_load_failed: policy parse failed for {path}: {err}");
                        eprintln!("{msg}");
                        errors.push(msg);
                    }
                },
                Err(err) => {
                    let msg = format!(
                        "policy_load_failed: policy file not found or unreadable: {path}: {err}"
                    );
                    eprintln!("{msg}");
                    errors.push(msg);
                }
            }
        }
    }

    let explicit_policy_dir = env::var("NC_CONTRACT_POLICY_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let policy_dir = explicit_policy_dir
        .clone()
        .unwrap_or_else(|| "contracts".to_string());
    match fs::read_dir(&policy_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let policy_path = path.join("policy.json");
                    if policy_path.exists() {
                        match fs::read_to_string(&policy_path) {
                            Ok(data) => match serde_json::from_str::<ContractPolicy>(&data) {
                                Ok(policy) => policies.push(policy),
                                Err(err) => {
                                    let msg = format!(
                                        "policy_load_failed: policy parse failed for {}: {err}",
                                        policy_path.display()
                                    );
                                    eprintln!("{msg}");
                                    errors.push(msg);
                                }
                            },
                            Err(err) => {
                                let msg = format!(
                                    "policy_load_failed: policy file not readable: {}: {err}",
                                    policy_path.display()
                                );
                                eprintln!("{msg}");
                                errors.push(msg);
                            }
                        }
                    }
                }
            }
        }
        Err(err) => {
            if explicit_policy_dir.is_some() {
                let msg = format!(
                    "policy_load_failed: policy dir not found or unreadable: {policy_dir}: {err}"
                );
                eprintln!("{msg}");
                errors.push(msg);
            }
        }
    }

    ContractPolicyLoad { policies, errors }
}

fn plan_needs_contract_policy(plan: &ActionPlan) -> bool {
    plan.actions.iter().any(|action| {
        matches!(
            action,
            Action::SorobanContractInvoke { .. } | Action::SorobanContractDeploy { .. }
        )
    })
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
        x402_stellar: Arc::new(Mutex::new(build_x402_challenge_store())),
        x402_payment_verifier: Arc::from(build_x402_payment_verifier()),
    });

    let api = Router::new()
        .route("/analyze", post(api_analyze))
        .route("/stellar/intent-plan", post(api_stellar_intent_plan))
        .route(
            "/stellar/zk-attestation/view",
            post(api_stellar_zk_attestation_view),
        )
        .route(
            "/x402/stellar/intent-plan",
            post(api_x402_stellar_intent_plan),
        )
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
                    requires_approval: false,
                    plan: ActionPlan::default(),
                    logs,
                }),
            );
        }
    }

    build_stellar_intent_plan_response(req, logs)
}

async fn api_stellar_zk_attestation_view(
    headers: HeaderMap,
    Json(req): Json<ZkAttestationViewRequest>,
) -> Response {
    let mut logs = vec!["zk_attestation: read-only public artifact view".to_string()];
    if let Some(required) = required_api_key() {
        let ok = provided_api_key(&headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            logs.push("auth: missing or invalid api key".to_string());
            return (
                StatusCode::UNAUTHORIZED,
                Json(ZkAttestationViewResponse::failure("unauthorized", logs)),
            )
                .into_response();
        }
    }

    match inspect_zk_attestation(req) {
        Ok(mut response) => {
            response.logs.splice(0..0, logs);
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(error) => {
            logs.push(format!("zk_attestation: rejected code={}", error.code()));
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ZkAttestationViewResponse::failure(error.code(), logs)),
            )
                .into_response()
        }
    }
}

async fn api_x402_stellar_intent_plan(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<StellarIntentPlanReq>,
) -> Response {
    let mut logs: Vec<String> = vec!["x402: stellar intent-plan gateway".to_string()];
    logs.push(format!(
        "x402_verifier: {}",
        state.x402_payment_verifier.verifier_kind()
    ));
    logs.push(format!(
        "x402_facilitator_boundary: {}",
        state.x402_payment_verifier.boundary_kind()
    ));

    if let Some(required) = required_api_key() {
        let ok = provided_api_key(&headers)
            .map(|got| secure_eq(got, required))
            .unwrap_or(false);
        if !ok {
            logs.push("auth: missing or invalid api key".to_string());
            return x402_error_response(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "unauthorized",
                X402PaymentContext::default(),
                logs,
            );
        }
    }

    let Some(signature) = x402_payment_signature(&headers) else {
        let record = match state.x402_stellar.lock() {
            Ok(mut store) => {
                let store_kind = store.store_kind();
                match state.x402_payment_verifier.create_challenge(store.as_mut()) {
                    Ok(record) => {
                        logs.push(format!("x402_store: {store_kind}"));
                        record
                    }
                    Err(err) => {
                        logs.push(format!("x402: challenge store create failed: {err}"));
                        return x402_error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "x402_state_unavailable",
                            "state_unavailable",
                            X402PaymentContext::default(),
                            logs,
                        );
                    }
                }
            }
            Err(_) => {
                logs.push("x402: state lock poisoned".to_string());
                return x402_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "x402_state_unavailable",
                    "state_unavailable",
                    X402PaymentContext::default(),
                    logs,
                );
            }
        };
        logs.push("x402: payment required".to_string());
        logs.push("x402: retry with PAYMENT-SIGNATURE=paid:<challenge_id>".to_string());
        return x402_payment_required_response(
            record.challenge_id,
            record.challenge.created_at,
            record.challenge.expires_at,
            logs,
        );
    };

    let (challenge_id, created_at, expires_at, finalized_at, payment_state) =
        match state.x402_stellar.lock() {
            Ok(mut store) => {
                let store_kind = store.store_kind();
                logs.push(format!("x402_store: {store_kind}"));
                match state
                    .x402_payment_verifier
                    .verify_and_finalize(&signature, store.as_mut())
                {
                    Ok(X402PaymentVerification::InvalidPayment) => {
                        logs.push("x402: invalid payment proof".to_string());
                        return x402_error_response(
                            StatusCode::PAYMENT_REQUIRED,
                            "invalid_payment",
                            "invalid",
                            X402PaymentContext::default(),
                            logs,
                        );
                    }
                    Ok(X402PaymentVerification::ReplayBlocked {
                        challenge_id,
                        challenge,
                    }) => {
                        logs.push(format!("x402: replay blocked for challenge={challenge_id}"));
                        return x402_error_response(
                            StatusCode::CONFLICT,
                            "payment_replay_blocked",
                            &challenge.payment_state,
                            X402PaymentContext {
                                challenge_id: Some(&challenge_id),
                                created_at: Some(challenge.created_at),
                                expires_at: Some(challenge.expires_at),
                                finalized_at: challenge.finalized_at,
                            },
                            logs,
                        );
                    }
                    Ok(X402PaymentVerification::Expired {
                        challenge_id,
                        challenge,
                    }) => {
                        logs.push(format!("x402: expired challenge={challenge_id}"));
                        return x402_error_response(
                            StatusCode::PAYMENT_REQUIRED,
                            "payment_expired",
                            &challenge.payment_state,
                            X402PaymentContext {
                                challenge_id: Some(&challenge_id),
                                created_at: Some(challenge.created_at),
                                expires_at: Some(challenge.expires_at),
                                finalized_at: challenge.finalized_at,
                            },
                            logs,
                        );
                    }
                    Ok(X402PaymentVerification::Finalized {
                        challenge_id,
                        challenge,
                    }) => (
                        challenge_id,
                        challenge.created_at,
                        challenge.expires_at,
                        challenge.finalized_at.unwrap_or_else(now_unix_secs),
                        challenge.payment_state,
                    ),
                    Err(err) => {
                        logs.push(format!("x402: challenge store finalize failed: {err}"));
                        return x402_error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "x402_state_unavailable",
                            "state_unavailable",
                            X402PaymentContext::default(),
                            logs,
                        );
                    }
                }
            }
            Err(_) => {
                logs.push("x402: state lock poisoned".to_string());
                return x402_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "x402_state_unavailable",
                    "state_unavailable",
                    X402PaymentContext::default(),
                    logs,
                );
            }
        };

    logs.push(format!("x402: finalized challenge={challenge_id}"));
    let (_status, Json(resp)) = build_stellar_intent_plan_response(req, logs);
    let outcome = X402StellarIntentPlanOutcome {
        ok: resp.ok,
        blocked: resp.blocked,
        requires_approval: resp.requires_approval,
        exit_code: resp.exit_code,
        error: resp.error,
        plan: resp.plan,
        logs: resp.logs,
    };
    x402_stellar_decision_response(
        &challenge_id,
        created_at,
        expires_at,
        finalized_at,
        &payment_state,
        outcome,
    )
}

fn build_stellar_intent_plan_response(
    req: StellarIntentPlanReq,
    mut logs: Vec<String>,
) -> (StatusCode, Json<StellarIntentPlanResp>) {
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
                requires_approval: false,
                plan: ActionPlan::default(),
                logs,
            }),
        );
    }

    let model_path = match resolve_stellar_intent_model_path(&req, &mut logs) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::OK,
                Json(StellarIntentPlanResp {
                    ok: false,
                    blocked: true,
                    exit_code: Some(2),
                    error: Some(err),
                    requires_approval: false,
                    plan: ActionPlan::default(),
                    logs,
                }),
            );
        }
    };
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
                        requires_approval: false,
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
                    requires_approval: false,
                    plan: ActionPlan::default(),
                    logs,
                }),
            );
        }
    };

    let policy_load = load_contract_policies();
    let policies = &policy_load.policies;
    let mut plan = build_intent_action_plan(&prompt, &decision);
    plan.warnings
        .push(format!("intent_model: path={model_path}"));
    let (template_warnings, template_errors) =
        soroban_deep::validate_contract_policy_templates(policies);
    logs.push(format!(
        "policy_load: policies={} errors={}",
        policies.len(),
        policy_load.errors.len()
    ));
    for err in &policy_load.errors {
        plan.warnings.push(format!("policy error: {err}"));
    }
    logs.push(format!(
        "policy_template: warnings={} errors={}",
        template_warnings.len(),
        template_errors.len()
    ));
    for warning in &template_warnings {
        plan.warnings
            .push(format!("policy_template warning: {warning}"));
    }
    for err in &template_errors {
        plan.warnings.push(format!("policy_template error: {err}"));
    }
    let template_report =
        soroban_deep::apply_contract_intent_templates(&prompt, &mut plan, policies);
    logs.push(format!(
        "soroban_deep_template: expanded={} template={} contract_id={} function={} reason={}",
        template_report.expanded,
        template_report.template_name.as_deref().unwrap_or("(none)"),
        template_report.contract_id.as_deref().unwrap_or("(none)"),
        template_report.function.as_deref().unwrap_or("(none)"),
        template_report.reason.as_deref().unwrap_or("(none)")
    ));
    let typed_v2_report = soroban_deep::apply_policy_typed_templates_v2(&mut plan, policies);
    logs.push(format!(
        "typed_template_v2: policy_slot_type_converted={} normalized_args={}",
        typed_v2_report.converted, typed_v2_report.normalized_args
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
    let allowlist_is_enforced = allowlist_enforced(req.allowlist_enforce);
    let allowlist_violations = if allowlist_is_enforced {
        validate_enforced_plan(&plan, &allowlist)
    } else {
        validate_plan(&plan, &allowlist)
    };
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

    let (policy_warnings, mut policy_errors) =
        soroban_deep::validate_contract_policies(&plan, policies);
    let policy_is_enforced = policy_enforced(req.contract_policy_enforce);
    if policy_is_enforced && plan_needs_contract_policy(&plan) {
        policy_errors.extend(policy_load.errors.iter().cloned());
        if policies.is_empty() {
            policy_errors.push(
                "policy_unconfigured: contract_policy_enforce enabled but no contract policies loaded"
                    .to_string(),
            );
        }
    }
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
    let requires_approval = req.requires_approval.unwrap_or(false) && !blocked;
    if requires_approval {
        plan.warnings
            .push("approval required before any submit or signing boundary".to_string());
        logs.push("approval: requires_approval boundary".to_string());
    }

    (
        StatusCode::OK,
        Json(StellarIntentPlanResp {
            ok: !blocked,
            blocked,
            exit_code,
            error: None,
            requires_approval,
            plan,
            logs,
        }),
    )
}
