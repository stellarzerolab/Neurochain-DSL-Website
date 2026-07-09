use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::x402_store::now_unix_secs;

pub fn write_x402_audit_event(
    logs: &mut Vec<String>,
    event: &str,
    http_status: StatusCode,
    audit_id: &str,
    payment: &Value,
    decision: &Value,
    guardrails: &Value,
) {
    let Some(path) = x402_stellar_audit_path() else {
        return;
    };

    if let Some(parent) = Path::new(&path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if let Err(err) = fs::create_dir_all(parent) {
            logs.push(format!("x402_audit: mkdir_failed {err}"));
            return;
        }
    }

    let row = json!({
        "schema_version": 1,
        "service": "stellar.intent_plan",
        "endpoint": "/api/x402/stellar/intent-plan",
        "event": event,
        "timestamp": now_unix_secs(),
        "http_status": http_status.as_u16(),
        "audit_id": audit_id,
        "payment": payment,
        "decision": decision,
        "guardrails": guardrails
    });

    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            if let Err(err) = writeln!(file, "{row}") {
                logs.push(format!("x402_audit: write_failed {err}"));
            } else {
                logs.push("x402_audit: wrote safe event".to_string());
            }
        }
        Err(err) => logs.push(format!("x402_audit: open_failed {err}")),
    }
}

fn x402_stellar_audit_path() -> Option<String> {
    env::var("NC_X402_STELLAR_AUDIT_PATH")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
