use axum::{
    extract::{ConnectInfo, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};
use tower_http::cors::{Any, CorsLayer};

use neurochain::banner;
use neurochain::engine;
use neurochain::interpreter;

use std::panic::{catch_unwind, AssertUnwindSafe};

use tokio::{
    sync::{Mutex, OwnedSemaphorePermit, Semaphore},
    task,
    time::{timeout, Duration},
};

/* -------------------------- App state -------------------------- */

struct AppState {
    /// Global inference permits to cap concurrent analyses.
    inference_sem: Arc<Semaphore>,
    /// Optional API key for requests (NC_API_KEY).
    api_key: Option<String>,
    /// Per-IP inference permits to prevent one IP from consuming all slots.
    per_ip: Mutex<HashMap<IpAddr, IpBucket>>,
    per_ip_max: usize,
    ip_bucket_ttl: Duration,
    ip_cleanup_counter: AtomicUsize,
}

struct IpBucket {
    sem: Arc<Semaphore>,
    last_seen: Instant,
}

const IP_TABLE_CLEANUP_EVERY: usize = 256;

fn forwarded_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let xff = headers.get("x-forwarded-for")?.to_str().ok()?;
    for part in xff.split(',').rev() {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Ok(ip) = part.parse::<IpAddr>() {
            return Some(ip);
        }
        if let Ok(sa) = part.parse::<SocketAddr>() {
            return Some(sa.ip());
        }
    }
    None
}

fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> Option<IpAddr> {
    if let Some(ip) = forwarded_client_ip(headers) {
        return Some(ip);
    }

    let peer_ip = peer.ip();
    if peer_ip.is_loopback() || peer_ip.is_unspecified() {
        return None;
    }

    Some(peer_ip)
}

fn api_key_matches(headers: &HeaderMap, expected: &str) -> bool {
    if let Some(value) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        if value.trim() == expected {
            return true;
        }
    }

    if let Some(auth) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        let auth = auth.trim();
        if let Some(token) = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))
        {
            if token.trim() == expected {
                return true;
            }
        }
    }

    false
}

impl AppState {
    async fn acquire_per_ip_permit(&self, ip: IpAddr) -> Option<OwnedSemaphorePermit> {
        let sem = {
            let mut table = self.per_ip.lock().await;

            let now = Instant::now();
            self.maybe_cleanup_ip_table(&mut table, now);

            let bucket = table.entry(ip).or_insert_with(|| IpBucket {
                sem: Arc::new(Semaphore::new(self.per_ip_max)),
                last_seen: now,
            });
            bucket.last_seen = now;

            bucket.sem.clone()
        };

        sem.try_acquire_owned().ok()
    }

    fn maybe_cleanup_ip_table(&self, table: &mut HashMap<IpAddr, IpBucket>, now: Instant) {
        let n = self.ip_cleanup_counter.fetch_add(1, Ordering::Relaxed);
        if !n.is_multiple_of(IP_TABLE_CLEANUP_EVERY) {
            return;
        }

        let ttl = self.ip_bucket_ttl;
        let per_ip_max = self.per_ip_max;
        table.retain(|_, bucket| {
            if now.duration_since(bucket.last_seen) <= ttl {
                true
            } else {
                bucket.sem.available_permits() != per_ip_max
            }
        });
    }
}

/* -------------------------- Request/Response ------------------- */
/* WebUI may send 'code' or 'content' (analyze), and 'prompt' or 'content' (generate). */

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
struct GenerateReq {
    #[serde(default)]
    model: String,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    content: Option<String>,
}
#[derive(Serialize)]
struct GenerateResp {
    ok: bool,
    dsl: String,
    logs: Vec<String>,
}

/* -------------------------- Model mapping ---------------------- */

/// Base path for models. Override via NC_MODELS_DIR (default: /opt/neurochain/models).
fn models_base() -> String {
    env::var("NC_MODELS_DIR").unwrap_or_else(|_| "/opt/neurochain/models".to_string())
}

/// Map WebUI model IDs -> ONNX paths.
fn resolve_model_path(id: &str) -> Option<String> {
    let base = models_base();
    let p = match id {
        "sst2" => format!("{base}/distilbert-sst2/model.onnx"),
        "factcheck" => format!("{base}/factcheck/model.onnx"),
        "intent" => format!("{base}/intent/model.onnx"),
        "toxic" => format!("{base}/toxic_quantized/model.onnx"),

        // MacroIntent aliases
        "macro" | "intent_macro" | "macro_intent" | "gpt2" | "generator" => {
            format!("{base}/intent_macro/model.onnx")
        }

        // Optional: snake custom policy shortcut (if you want to pass model:"policy")
        "policy" => format!("{base}/policy/model.onnx"),

        _ => return None,
    };
    Some(p)
}

/* -------------------------- Utility ---------------------------- */

/// Normalize line endings and tabs before parsing (BOM/CRLF -> LF, tabs -> 4 spaces).
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

/* -------------------------- Server main ------------------------ */

#[tokio::main]
async fn main() {
    banner::print_banner();

    // Optional panic hook: more visible logs in journald.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("PANIC: {info}");
        if std::env::var("RUST_BACKTRACE").as_deref() != Ok("0") {
            eprintln!("(enable RUST_BACKTRACE=1 for backtrace)");
        }
    }));

    // Max concurrent inferences (env NC_MAX_INFER, default 2).
    let max_infer: usize = env::var("NC_MAX_INFER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2)
        .max(1);

    // Per-IP max concurrent inferences (env NC_MAX_INFER_PER_IP).
    let per_ip_default = max_infer.saturating_div(2).max(1);
    let per_ip_max: usize = env::var("NC_MAX_INFER_PER_IP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(per_ip_default)
        .clamp(1, max_infer);

    // Per-IP table TTL (env NC_IP_BUCKET_TTL_SECS, default 3600s).
    let ip_bucket_ttl_secs: u64 = env::var("NC_IP_BUCKET_TTL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600);

    let api_key = env::var("NC_API_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let state = Arc::new(AppState {
        inference_sem: Arc::new(Semaphore::new(max_infer)),
        api_key,
        per_ip: Mutex::new(HashMap::new()),
        per_ip_max,
        ip_bucket_ttl: Duration::from_secs(ip_bucket_ttl_secs),
        ip_cleanup_counter: AtomicUsize::new(0),
    });

    let api = Router::new()
        .route("/analyze", post(api_analyze))
        .route("/generate", post(api_generate))
        .with_state(state);

    // API only; static files are served by Apache.
    let app = Router::new().nest("/api", api).layer(
        CorsLayer::new()
            .allow_origin(Any) // reverse proxy -> same-origin; safe to keep here
            .allow_methods(Any)
            .allow_headers(Any),
    );

    // Default: 127.0.0.1:8081 (behind Apache proxy).
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8081);

    let addr: SocketAddr = format!("{host}:{port}").parse().expect("Invalid HOST/PORT");
    println!("✅ NeuroChain API listening on http://{addr}");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("❌ Bind failed for address {addr}: {e}");
            eprintln!(
                "   Hint: is the port already in use? e.g. `ss -tulpn | grep :{}`",
                port
            );
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        eprintln!("❌ Server error: {e}");
        std::process::exit(1);
    }
}

/* -------------------------- Handlers --------------------------- */

async fn api_analyze(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(req): Json<AnalyzeReq>,
) -> impl IntoResponse {
    let mut logs: Vec<String> = Vec::new();
    if !req.model.is_empty() {
        logs.push(format!("model={}", req.model));
    }

    if let Some(expected) = &s.api_key {
        if !api_key_matches(&headers, expected) {
            logs.push("auth: missing or invalid API key".into());
            return (
                StatusCode::UNAUTHORIZED,
                Json(AnalyzeResp {
                    ok: false,
                    output: "ERROR: missing or invalid API key".into(),
                    logs,
                }),
            );
        }
    }

    // Pick code (code > content > empty).
    let mut code = req.code.or(req.content).unwrap_or_default();

    if code.trim().is_empty() {
        logs.push("warn: empty input".into());
        return (
            StatusCode::OK,
            Json(AnalyzeResp {
                ok: false,
                output: "⚠️ Empty input".into(),
                logs,
            }),
        );
    }

    // Auto-inject AI line if missing and the model is known.
    if let Some(path) = resolve_model_path(&req.model) {
        let has_ai = code.lines().any(|l| l.trim_start().starts_with("AI:"));
        if !has_ai {
            code = format!("AI: \"{path}\"\n{code}");
            logs.push(format!("auto: injected AI model path {}", path));
        }
    } else if !req.model.is_empty() {
        logs.push(format!("warn: unknown model id '{}'", req.model));
    }

    // Critical: normalize before parsing (BOM/CRLF/tabs).
    let code = normalize(&code);

    // Per-IP gate: prevent one IP from consuming all slots.
    // Note: if we cannot obtain a reliable IP (e.g. peer=127.0.0.1 without XFF),
    // skip per-IP limiting so users don't all share the same bucket.
    let per_ip_permit = if let Some(ip) = client_ip(&headers, peer) {
        match s.acquire_per_ip_permit(ip).await {
            Some(p) => Some(p),
            None => {
                logs.push("busy: per-ip limit reached".into());
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(AnalyzeResp {
                        ok: false,
                        output: "⚠️ Too many concurrent requests from your IP — please wait a moment and try again."
                            .into(),
                        logs,
                    }),
                );
            }
        }
    } else {
        None
    };

    // CPU gate:
    // 1) Exit fast: if all permits are used, return 503 and do not block.
    // 2) Small "soft wait": if permits free up, wait up to 50 ms.
    let permit = match s.inference_sem.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            // Try a short wait if permits are about to free up.
            let maybe = timeout(
                Duration::from_millis(50),
                s.inference_sem.clone().acquire_owned(),
            )
            .await;
            match maybe {
                Ok(Ok(p)) => p,
                _ => {
                    logs.push("busy: inference slots full".into());
                    return (StatusCode::SERVICE_UNAVAILABLE, Json(AnalyzeResp {
                        ok: false,
                        output: "⚠️ Too many users right now — thank you for your patience. In the meantime, try the local WebUI API.".into(),
                        logs,
                    }));
                }
            }
        }
    };

    // Run heavy work in a blocking thread (spawn_blocking) and guard against panics.
    // Move only 'code' into the closure; keep 'logs' in the handler.
    let task_res = task::spawn_blocking(move || {
        catch_unwind(AssertUnwindSafe(|| {
            let mut interp = interpreter::Interpreter::new();
            engine::analyze(&code, &mut interp)
        }))
    })
    .await;

    // Permit is released when 'permit' drops out of scope.
    drop(permit);
    drop(per_ip_permit);

    // Join error (tokio): task panicked or crashed.
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
            // Expose the actual panic message in JSON.
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

async fn api_generate(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<GenerateReq>,
) -> impl IntoResponse {
    let mut logs: Vec<String> = Vec::new();
    if !req.model.is_empty() {
        logs.push(format!("model={}", req.model));
    }

    if let Some(expected) = &s.api_key {
        if !api_key_matches(&headers, expected) {
            logs.push("auth: missing or invalid API key".into());
            return (
                StatusCode::UNAUTHORIZED,
                Json(GenerateResp {
                    ok: false,
                    dsl: "# ERROR: missing or invalid API key".into(),
                    logs,
                }),
            );
        }
    }

    // prompt > content > empty
    let prompt = req.prompt.or(req.content).unwrap_or_default();

    if prompt.trim().is_empty() {
        logs.push("warn: empty prompt".into());
        return (
            StatusCode::OK,
            Json(GenerateResp {
                ok: false,
                dsl: "# ERROR: empty prompt".into(),
                logs,
            }),
        );
    }

    // Optional normalization for the generator too.
    let prompt = normalize(&prompt);

    // Use engine::generate (stub).
    let (ok, dsl) = match engine::generate(&prompt) {
        Ok(res) => (true, res),
        Err(e) => (false, format!("# ERROR: {e}")),
    };

    (StatusCode::OK, Json(GenerateResp { ok, dsl, logs }))
}
