use std::env;

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::{actions::ActionPlan, x402_audit::write_x402_audit_event, x402_store::now_unix_secs};

#[derive(Debug, Clone, Copy, Default)]
pub struct X402PaymentContext<'a> {
    pub challenge_id: Option<&'a str>,
    pub created_at: Option<u64>,
    pub expires_at: Option<u64>,
    pub finalized_at: Option<u64>,
}

pub struct X402StellarIntentPlanOutcome {
    pub ok: bool,
    pub blocked: bool,
    pub requires_approval: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub plan: ActionPlan,
    pub logs: Vec<String>,
}

pub fn x402_payment_signature(headers: &HeaderMap) -> Option<String> {
    headers
        .get("payment-signature")
        .or_else(|| headers.get("x-payment"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn x402_challenge_from_signature(signature: &str) -> Option<&str> {
    signature.trim().strip_prefix("paid:").map(str::trim)
}

pub fn x402_payment_required_response(
    challenge_id: String,
    created_at: u64,
    expires_at: u64,
    mut logs: Vec<String>,
) -> Response {
    let audit_id = x402_audit_id(&challenge_id);
    let payment = x402_payment_json(
        "payment_required",
        Some(&challenge_id),
        Some(created_at),
        Some(expires_at),
        None,
    );
    let decision = json!({
        "status": "not_evaluated",
        "approved": false,
        "blocked": false,
        "requires_approval": false,
        "reason": null
    });
    let guardrails = json!({
        "state": "not_run",
        "exit_code": null,
        "reason": null
    });
    write_x402_audit_event(
        &mut logs,
        "payment_required",
        StatusCode::PAYMENT_REQUIRED,
        &audit_id,
        &payment,
        &decision,
        &guardrails,
    );

    (
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "ok": false,
            "blocked": false,
            "error": "payment_required",
            "audit_id": audit_id,
            "challenge_id": &challenge_id,
            "amount": x402_stellar_amount(),
            "asset": x402_stellar_asset(),
            "network": x402_stellar_network(),
            "receiver": x402_stellar_receiver(),
            "expires_at": expires_at,
            "payment_header": "PAYMENT-SIGNATURE",
            "mock_signature": format!("paid:{challenge_id}"),
            "payment": payment,
            "decision": decision,
            "guardrails": guardrails,
            "logs": logs
        })),
    )
        .into_response()
}

pub fn x402_error_response(
    status: StatusCode,
    error: &str,
    payment_state: &str,
    ctx: X402PaymentContext<'_>,
    mut logs: Vec<String>,
) -> Response {
    let audit_id = ctx
        .challenge_id
        .map(x402_audit_id)
        .unwrap_or_else(|| format!("x402-stellar-untracked-{}", now_unix_secs()));
    let payment = x402_payment_json(
        payment_state,
        ctx.challenge_id,
        ctx.created_at,
        ctx.expires_at,
        ctx.finalized_at,
    );
    let decision = json!({
        "status": "blocked",
        "approved": false,
        "blocked": true,
        "requires_approval": false,
        "reason": error
    });
    let guardrails = json!({
        "state": "not_run",
        "exit_code": null,
        "reason": null
    });
    write_x402_audit_event(
        &mut logs,
        error,
        status,
        &audit_id,
        &payment,
        &decision,
        &guardrails,
    );

    (
        status,
        Json(json!({
            "ok": false,
            "blocked": true,
            "error": error,
            "audit_id": audit_id,
            "payment": payment,
            "decision": decision,
            "guardrails": guardrails,
            "logs": logs
        })),
    )
        .into_response()
}

pub fn x402_stellar_decision_response(
    challenge_id: &str,
    created_at: u64,
    expires_at: u64,
    finalized_at: u64,
    payment_state: &str,
    outcome: X402StellarIntentPlanOutcome,
) -> Response {
    let decision_status = if outcome.blocked {
        "blocked"
    } else if outcome.requires_approval {
        "requires_approval"
    } else {
        "approved"
    };
    let guardrail_state = if outcome.blocked { "blocked" } else { "passed" };
    let guardrail_reason = x402_guardrail_reason(outcome.exit_code, outcome.error.as_deref());
    let decision_reason = if outcome.requires_approval {
        Some("approval_required".to_string())
    } else {
        guardrail_reason.clone()
    };
    let audit_id = x402_audit_id(challenge_id);
    let payment = x402_payment_json(
        payment_state,
        Some(challenge_id),
        Some(created_at),
        Some(expires_at),
        Some(finalized_at),
    );
    let decision = json!({
        "status": decision_status,
        "approved": outcome.ok && !outcome.blocked && !outcome.requires_approval,
        "blocked": outcome.blocked,
        "requires_approval": outcome.requires_approval,
        "reason": decision_reason
    });
    let guardrails = json!({
        "state": guardrail_state,
        "exit_code": outcome.exit_code,
        "reason": guardrail_reason
    });
    let mut logs = outcome.logs;
    write_x402_audit_event(
        &mut logs,
        decision_status,
        StatusCode::OK,
        &audit_id,
        &payment,
        &decision,
        &guardrails,
    );

    (
        StatusCode::OK,
        Json(json!({
            "ok": outcome.ok,
            "blocked": outcome.blocked,
            "exit_code": outcome.exit_code,
            "error": outcome.error,
            "audit_id": audit_id,
            "payment": payment,
            "decision": decision,
            "guardrails": guardrails,
            "plan": outcome.plan,
            "logs": logs
        })),
    )
        .into_response()
}

fn x402_audit_id(challenge_id: &str) -> String {
    format!("x402-stellar-{challenge_id}")
}

fn x402_guardrail_reason(exit_code: Option<i32>, error: Option<&str>) -> Option<String> {
    match exit_code {
        Some(3) => Some("allowlist".to_string()),
        Some(4) => Some("contract_policy".to_string()),
        Some(5) => Some("intent_safety".to_string()),
        Some(code) => Some(format!("exit_code_{code}")),
        None => error.map(str::to_string),
    }
}

fn x402_payment_json(
    state: &str,
    challenge_id: Option<&str>,
    created_at: Option<u64>,
    expires_at: Option<u64>,
    finalized_at: Option<u64>,
) -> Value {
    json!({
        "protocol": "x402",
        "state": state,
        "challenge_id": challenge_id,
        "amount": x402_stellar_amount(),
        "asset": x402_stellar_asset(),
        "network": x402_stellar_network(),
        "receiver": x402_stellar_receiver(),
        "created_at": created_at,
        "expires_at": expires_at,
        "finalized_at": finalized_at
    })
}

fn x402_stellar_amount() -> String {
    env::var("NC_X402_STELLAR_AMOUNT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "0.01".to_string())
}

fn x402_stellar_asset() -> String {
    env::var("NC_X402_STELLAR_ASSET")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "USDC".to_string())
}

fn x402_stellar_network() -> String {
    env::var("NC_X402_STELLAR_NETWORK")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "stellar:testnet".to_string())
}

fn x402_stellar_receiver() -> String {
    env::var("NC_X402_STELLAR_RECEIVER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "mock-receiver".to_string())
}
