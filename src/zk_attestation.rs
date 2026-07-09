use neurochain_zk_guardrail_contract::{
    DecisionStatus, ExitCode, PublicJournal, ReasonCode, TypedActionPlan, TypedArg, TypedValue,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZkTypedActionPlan {
    pub schema_version: u32,
    pub intent_label: String,
    pub action_kind: String,
    pub contract_id: String,
    pub function: String,
    pub args: Vec<ZkTypedArg>,
    pub intent_confidence_bps: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZkTypedArg {
    pub name: String,
    #[serde(rename = "type")]
    pub value_type: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZkProofArtifact {
    pub schema_version: u32,
    pub seal_hex: String,
    pub image_id_hex: String,
    pub journal_hex: String,
    pub journal_digest_hex: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZkAttestationViewRequest {
    pub action_plan: ZkTypedActionPlan,
    pub proof: ZkProofArtifact,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkAttestedDecision {
    pub status: String,
    pub exit_code: u8,
    pub reason: String,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkAttestationView {
    pub verification_state: String,
    pub cryptographically_verified: bool,
    pub stellar_verification_required: bool,
    pub proof_kind: String,
    pub verifier_selector: String,
    pub evaluator_image_id: String,
    pub action_plan_hash: String,
    pub policy_commitment: String,
    pub policy_version: u32,
    pub audit_nullifier: String,
    pub private_policy_revealed: bool,
    pub attested_decision: ZkAttestedDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkExecutionView {
    pub state: String,
    pub submit_allowed: bool,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkAttestationViewResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_plan: Option<ZkTypedActionPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zk_attestation: Option<ZkAttestationView>,
    pub execution: ZkExecutionView,
    pub logs: Vec<String>,
}

impl ZkAttestationViewResponse {
    pub fn failure(code: &str, mut logs: Vec<String>) -> Self {
        logs.push(format!("submit: blocked reason={code}"));
        Self {
            ok: false,
            error: Some(code.to_string()),
            action_plan: None,
            zk_attestation: None,
            execution: ZkExecutionView {
                state: "blocked".to_string(),
                submit_allowed: false,
                reason: code.to_string(),
                next_step: None,
            },
            logs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZkAttestationViewError {
    code: &'static str,
}

impl ZkAttestationViewError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub fn code(self) -> &'static str {
        self.code
    }
}

pub fn inspect_zk_attestation(
    request: ZkAttestationViewRequest,
) -> Result<ZkAttestationViewResponse, ZkAttestationViewError> {
    if request.proof.schema_version != 1 {
        return Err(ZkAttestationViewError::new("unsupported_proof_schema"));
    }

    let seal = decode_hex(&request.proof.seal_hex, "invalid_seal")?;
    if seal.len() <= 4 {
        return Err(ZkAttestationViewError::new("invalid_seal"));
    }
    let image_id = decode_digest(&request.proof.image_id_hex, "invalid_image_id")?;
    let journal_bytes = decode_hex(&request.proof.journal_hex, "invalid_public_journal")?;
    let declared_journal_digest =
        decode_digest(&request.proof.journal_digest_hex, "invalid_journal_digest")?;

    let actual_journal_digest: [u8; 32] = Sha256::digest(&journal_bytes).into();
    if actual_journal_digest != declared_journal_digest {
        return Err(ZkAttestationViewError::new("journal_digest_mismatch"));
    }

    let journal = PublicJournal::decode(&journal_bytes)
        .map_err(|_| ZkAttestationViewError::new("invalid_public_journal"))?;
    if journal.evaluator_image_id != image_id {
        return Err(ZkAttestationViewError::new("evaluator_image_id_mismatch"));
    }

    let action_plan_hash = canonical_action_plan_hash(&request.action_plan)?;
    if journal.action_plan_hash != action_plan_hash {
        return Err(ZkAttestationViewError::new("action_plan_hash_mismatch"));
    }

    Ok(ZkAttestationViewResponse {
        ok: true,
        error: None,
        action_plan: Some(request.action_plan),
        zk_attestation: Some(ZkAttestationView {
            verification_state: "binding_validated".to_string(),
            cryptographically_verified: false,
            stellar_verification_required: true,
            proof_kind: "groth16".to_string(),
            verifier_selector: hex::encode(&seal[..4]),
            evaluator_image_id: hex::encode(journal.evaluator_image_id),
            action_plan_hash: hex::encode(journal.action_plan_hash),
            policy_commitment: hex::encode(journal.policy_commitment),
            policy_version: journal.policy_version,
            audit_nullifier: hex::encode(journal.audit_nullifier),
            private_policy_revealed: false,
            attested_decision: ZkAttestedDecision {
                status: decision_status_name(journal.decision_status).to_string(),
                exit_code: exit_code_number(journal.exit_code),
                reason: reason_code_name(journal.reason_code).to_string(),
                requires_approval: journal.requires_approval,
            },
        }),
        execution: ZkExecutionView {
            state: "blocked".to_string(),
            submit_allowed: false,
            reason: "stellar_verification_required".to_string(),
            next_step: Some("verify_on_stellar_then_separate_approval".to_string()),
        },
        logs: vec![
            "zk_attestation: public journal decoded".to_string(),
            "binding: action_plan_hash matched".to_string(),
            "binding: evaluator_image_id matched".to_string(),
            "verification: Stellar cryptographic verification required".to_string(),
            "submit: blocked pending Stellar verification and separate approval".to_string(),
        ],
    })
}

fn canonical_action_plan_hash(
    action_plan: &ZkTypedActionPlan,
) -> Result<[u8; 32], ZkAttestationViewError> {
    let byte_values = action_plan
        .args
        .iter()
        .map(|arg| {
            if arg.value_type == "bytes" {
                hex::decode(&arg.value)
                    .map(Some)
                    .map_err(|_| ZkAttestationViewError::new("invalid_action_plan"))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    let typed_args = action_plan
        .args
        .iter()
        .zip(byte_values.iter())
        .map(|(arg, bytes)| {
            let value = match arg.value_type.as_str() {
                "address" => TypedValue::Address(&arg.value),
                "bytes" => TypedValue::Bytes(
                    bytes
                        .as_deref()
                        .ok_or_else(|| ZkAttestationViewError::new("invalid_action_plan"))?,
                ),
                "symbol" => TypedValue::Symbol(&arg.value),
                "u64" => TypedValue::U64(
                    arg.value
                        .parse()
                        .map_err(|_| ZkAttestationViewError::new("invalid_action_plan"))?,
                ),
                _ => return Err(ZkAttestationViewError::new("invalid_action_plan")),
            };
            Ok(TypedArg {
                name: &arg.name,
                value,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let typed_plan = TypedActionPlan {
        schema_version: action_plan.schema_version,
        intent_label: &action_plan.intent_label,
        action_kind: &action_plan.action_kind,
        contract_id: &action_plan.contract_id,
        function: &action_plan.function,
        args: &typed_args,
        intent_confidence_bps: action_plan.intent_confidence_bps,
    };
    let preimage = typed_plan
        .canonical_preimage()
        .map_err(|_| ZkAttestationViewError::new("invalid_action_plan"))?;
    Ok(Sha256::digest(preimage).into())
}

fn decode_hex(value: &str, code: &'static str) -> Result<Vec<u8>, ZkAttestationViewError> {
    hex::decode(value).map_err(|_| ZkAttestationViewError::new(code))
}

fn decode_digest(value: &str, code: &'static str) -> Result<[u8; 32], ZkAttestationViewError> {
    let bytes = decode_hex(value, code)?;
    bytes
        .try_into()
        .map_err(|_| ZkAttestationViewError::new(code))
}

fn decision_status_name(value: DecisionStatus) -> &'static str {
    match value {
        DecisionStatus::Approved => "approved",
        DecisionStatus::Blocked => "blocked",
        DecisionStatus::RequiresApproval => "requires_approval",
    }
}

fn exit_code_number(value: ExitCode) -> u8 {
    match value {
        ExitCode::Passed => 0,
        ExitCode::Allowlist => 3,
        ExitCode::ContractPolicy => 4,
        ExitCode::IntentSafety => 5,
    }
}

fn reason_code_name(value: ReasonCode) -> &'static str {
    match value {
        ReasonCode::Passed => "passed",
        ReasonCode::Allowlist => "allowlist",
        ReasonCode::ContractPolicy => "contract_policy",
        ReasonCode::IntentSafety => "intent_safety",
        ReasonCode::ApprovalThreshold => "approval_threshold",
        ReasonCode::InvalidAttestation => "invalid_attestation",
        ReasonCode::Replay => "replay",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approved_request() -> ZkAttestationViewRequest {
        ZkAttestationViewRequest {
            action_plan: serde_json::from_str(include_str!(
                "../hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json"
            ))
            .expect("typed ActionPlan fixture"),
            proof: serde_json::from_str(include_str!(
                "../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
            ))
            .expect("Groth16 fixture"),
        }
    }

    #[test]
    fn genuine_artifact_binds_action_plan_but_never_grants_submit() {
        let response = inspect_zk_attestation(approved_request()).expect("binding must validate");
        let attestation = response.zk_attestation.expect("attestation view");

        assert!(response.ok);
        assert_eq!(attestation.verification_state, "binding_validated");
        assert!(!attestation.cryptographically_verified);
        assert!(attestation.stellar_verification_required);
        assert_eq!(attestation.attested_decision.status, "approved");
        assert_eq!(attestation.attested_decision.exit_code, 0);
        assert!(!response.execution.submit_allowed);
        assert_eq!(response.execution.state, "blocked");
    }

    #[test]
    fn requires_approval_artifact_stays_blocked_before_stellar_verification() {
        let mut request = approved_request();
        request.proof = serde_json::from_str(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_requires_approval.json"
        ))
        .expect("requires-approval Groth16 fixture");

        let response = inspect_zk_attestation(request).expect("binding must validate");
        let attestation = response.zk_attestation.expect("attestation view");

        assert_eq!(attestation.attested_decision.status, "requires_approval");
        assert_eq!(attestation.attested_decision.exit_code, 0);
        assert_eq!(attestation.attested_decision.reason, "approval_threshold");
        assert!(attestation.attested_decision.requires_approval);
        assert!(!response.execution.submit_allowed);
        assert_eq!(response.execution.state, "blocked");
    }

    #[test]
    fn allowlist_block_artifact_exposes_exit_3_without_granting_submit() {
        let mut request = approved_request();
        request.proof = serde_json::from_str(include_str!(
            "../hackathons/stellar-real-world-zk/fixtures/groth16_blocked_exit_3.json"
        ))
        .expect("exit-3 Groth16 fixture");

        let response = inspect_zk_attestation(request).expect("binding must validate");
        let attestation = response.zk_attestation.expect("attestation view");

        assert_eq!(attestation.attested_decision.status, "blocked");
        assert_eq!(attestation.attested_decision.exit_code, 3);
        assert_eq!(attestation.attested_decision.reason, "allowlist");
        assert!(!attestation.attested_decision.requires_approval);
        assert!(!response.execution.submit_allowed);
        assert_eq!(response.execution.state, "blocked");
    }

    #[test]
    fn changed_action_plan_fails_closed_before_any_execution_state() {
        let mut request = approved_request();
        request.action_plan.args[0].value = "500000001".to_string();

        let error = inspect_zk_attestation(request).expect_err("hash mismatch must reject");
        assert_eq!(error.code(), "action_plan_hash_mismatch");
        let response = ZkAttestationViewResponse::failure(error.code(), Vec::new());
        assert!(!response.ok);
        assert!(!response.execution.submit_allowed);
        assert!(response.zk_attestation.is_none());
    }

    #[test]
    fn changed_journal_digest_fails_closed() {
        let mut request = approved_request();
        request.proof.journal_digest_hex.replace_range(0..2, "00");

        let error = inspect_zk_attestation(request).expect_err("digest mismatch must reject");
        assert_eq!(error.code(), "journal_digest_mismatch");
    }
}
