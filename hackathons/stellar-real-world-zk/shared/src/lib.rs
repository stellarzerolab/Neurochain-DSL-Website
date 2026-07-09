#![cfg_attr(not(test), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::{format, vec::Vec};

pub const CONTRACT_VERSION: u32 = 1;
pub const ACTION_PLAN_DOMAIN: &[u8] = b"NC_ZK_ACTION_PLAN_V1";
pub const PRIVATE_POLICY_DOMAIN: &[u8] = b"NC_ZK_PRIVATE_POLICY_V1";
pub const PUBLIC_JOURNAL_DOMAIN: &[u8] = b"NC_ZK_PUBLIC_JOURNAL_V1";
pub const AUDIT_NULLIFIER_DOMAIN: &[u8] = b"NC_ZK_AUDIT_NULLIFIER_V1";
pub const PUBLIC_JOURNAL_ENCODED_LEN: usize =
    PUBLIC_JOURNAL_DOMAIN.len() + 1 + 4 + (32 * 3) + 4 + 4 + 32;

pub type Digest32 = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractError {
    UnsupportedVersion,
    InvalidIntentLabel,
    InvalidActionKind,
    EmptyRequiredField,
    NonCanonicalOrder,
    InvalidConfidence,
    InvalidPolicyThresholds,
    InvalidJournalSemantics,
    InvalidEncoding,
    InvalidEnumTag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedValue<'a> {
    Address(&'a str),
    Bytes(&'a [u8]),
    Symbol(&'a str),
    U64(u64),
}

impl TypedValue<'_> {
    #[cfg(feature = "alloc")]
    fn tag(self) -> u8 {
        match self {
            Self::Address(_) => 1,
            Self::Bytes(_) => 2,
            Self::Symbol(_) => 3,
            Self::U64(_) => 4,
        }
    }

    fn is_valid(self) -> bool {
        match self {
            Self::Address(value) | Self::Symbol(value) => !value.is_empty(),
            Self::Bytes(value) => !value.is_empty(),
            Self::U64(_) => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypedArg<'a> {
    pub name: &'a str,
    pub value: TypedValue<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypedActionPlan<'a> {
    pub schema_version: u32,
    pub intent_label: &'a str,
    pub action_kind: &'a str,
    pub contract_id: &'a str,
    pub function: &'a str,
    pub args: &'a [TypedArg<'a>],
    pub intent_confidence_bps: u16,
}

impl TypedActionPlan<'_> {
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        if self.schema_version != CONTRACT_VERSION {
            return Err(ContractError::UnsupportedVersion);
        }
        if self.intent_label != "ContractInvoke" {
            return Err(ContractError::InvalidIntentLabel);
        }
        if self.action_kind != "soroban_contract_invoke" {
            return Err(ContractError::InvalidActionKind);
        }
        if self.contract_id.is_empty() || self.function.is_empty() || self.args.is_empty() {
            return Err(ContractError::EmptyRequiredField);
        }
        if self.intent_confidence_bps > 10_000 {
            return Err(ContractError::InvalidConfidence);
        }
        if !self
            .args
            .iter()
            .all(|arg| !arg.name.is_empty() && arg.value.is_valid())
        {
            return Err(ContractError::EmptyRequiredField);
        }
        if !self.args.windows(2).all(|pair| pair[0].name < pair[1].name) {
            return Err(ContractError::NonCanonicalOrder);
        }
        Ok(())
    }

    #[cfg(feature = "alloc")]
    pub fn canonical_preimage(&self) -> Result<Vec<u8>, ContractError> {
        self.validate_shape()?;
        let mut out = Encoder::with_domain(ACTION_PLAN_DOMAIN);
        out.u32(self.schema_version);
        out.string(self.intent_label);
        out.string(self.action_kind);
        out.string(self.contract_id);
        out.string(self.function);
        out.u16(self.intent_confidence_bps);
        out.u32(self.args.len() as u32);
        for arg in self.args {
            out.string(arg.name);
            out.u8(arg.value.tag());
            match arg.value {
                TypedValue::Address(value) | TypedValue::Symbol(value) => out.string(value),
                TypedValue::Bytes(value) => out.bytes(value),
                TypedValue::U64(value) => out.u64(value),
            }
        }
        Ok(out.finish())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivatePolicy<'a> {
    pub schema_version: u32,
    pub policy_version: u32,
    pub commitment_salt: Digest32,
    pub allowed_contracts: &'a [&'a str],
    pub allowed_contract_functions: &'a [&'a str],
    pub allowed_assets: &'a [&'a str],
    pub allowed_recipients: &'a [&'a str],
    pub max_amount_minor: u64,
    pub approval_threshold_minor: u64,
    pub min_intent_confidence_bps: u16,
}

impl PrivatePolicy<'_> {
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        if self.schema_version != CONTRACT_VERSION || self.policy_version == 0 {
            return Err(ContractError::UnsupportedVersion);
        }
        if self.min_intent_confidence_bps > 10_000 {
            return Err(ContractError::InvalidConfidence);
        }
        if self.approval_threshold_minor > self.max_amount_minor {
            return Err(ContractError::InvalidPolicyThresholds);
        }
        for values in [
            self.allowed_contracts,
            self.allowed_contract_functions,
            self.allowed_assets,
            self.allowed_recipients,
        ] {
            if values.is_empty() || values.iter().any(|value| value.is_empty()) {
                return Err(ContractError::EmptyRequiredField);
            }
            if !values.windows(2).all(|pair| pair[0] < pair[1]) {
                return Err(ContractError::NonCanonicalOrder);
            }
        }
        Ok(())
    }

    #[cfg(feature = "alloc")]
    pub fn canonical_preimage(&self) -> Result<Vec<u8>, ContractError> {
        self.validate_shape()?;
        let mut out = Encoder::with_domain(PRIVATE_POLICY_DOMAIN);
        out.u32(self.schema_version);
        out.u32(self.policy_version);
        out.fixed32(&self.commitment_salt);
        out.strings(self.allowed_contracts);
        out.strings(self.allowed_contract_functions);
        out.strings(self.allowed_assets);
        out.strings(self.allowed_recipients);
        out.u64(self.max_amount_minor);
        out.u64(self.approval_threshold_minor);
        out.u16(self.min_intent_confidence_bps);
        Ok(out.finish())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DecisionStatus {
    Approved = 0,
    Blocked = 1,
    RequiresApproval = 2,
}

impl DecisionStatus {
    fn from_tag(tag: u8) -> Result<Self, ContractError> {
        match tag {
            0 => Ok(Self::Approved),
            1 => Ok(Self::Blocked),
            2 => Ok(Self::RequiresApproval),
            _ => Err(ContractError::InvalidEnumTag),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Passed = 0,
    Allowlist = 3,
    ContractPolicy = 4,
    IntentSafety = 5,
}

impl ExitCode {
    fn from_tag(tag: u8) -> Result<Self, ContractError> {
        match tag {
            0 => Ok(Self::Passed),
            3 => Ok(Self::Allowlist),
            4 => Ok(Self::ContractPolicy),
            5 => Ok(Self::IntentSafety),
            _ => Err(ContractError::InvalidEnumTag),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReasonCode {
    Passed = 0,
    Allowlist = 1,
    ContractPolicy = 2,
    IntentSafety = 3,
    ApprovalThreshold = 4,
    InvalidAttestation = 5,
    Replay = 6,
}

impl ReasonCode {
    fn from_tag(tag: u8) -> Result<Self, ContractError> {
        match tag {
            0 => Ok(Self::Passed),
            1 => Ok(Self::Allowlist),
            2 => Ok(Self::ContractPolicy),
            3 => Ok(Self::IntentSafety),
            4 => Ok(Self::ApprovalThreshold),
            5 => Ok(Self::InvalidAttestation),
            6 => Ok(Self::Replay),
            _ => Err(ContractError::InvalidEnumTag),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardrailDecision {
    pub decision_status: DecisionStatus,
    pub exit_code: ExitCode,
    pub reason_code: ReasonCode,
    pub requires_approval: bool,
}

impl GuardrailDecision {
    pub const fn approved() -> Self {
        Self {
            decision_status: DecisionStatus::Approved,
            exit_code: ExitCode::Passed,
            reason_code: ReasonCode::Passed,
            requires_approval: false,
        }
    }

    pub const fn requires_approval() -> Self {
        Self {
            decision_status: DecisionStatus::RequiresApproval,
            exit_code: ExitCode::Passed,
            reason_code: ReasonCode::ApprovalThreshold,
            requires_approval: true,
        }
    }

    pub const fn blocked(exit_code: ExitCode, reason_code: ReasonCode) -> Self {
        Self {
            decision_status: DecisionStatus::Blocked,
            exit_code,
            reason_code,
            requires_approval: false,
        }
    }

    pub fn into_journal(
        self,
        evaluator_image_id: Digest32,
        action_plan_hash: Digest32,
        policy_commitment: Digest32,
        policy_version: u32,
        audit_nullifier: Digest32,
    ) -> PublicJournal {
        PublicJournal {
            contract_version: CONTRACT_VERSION,
            evaluator_image_id,
            action_plan_hash,
            policy_commitment,
            policy_version,
            decision_status: self.decision_status,
            exit_code: self.exit_code,
            reason_code: self.reason_code,
            requires_approval: self.requires_approval,
            audit_nullifier,
        }
    }
}

#[cfg(feature = "alloc")]
pub fn evaluate(plan: &TypedActionPlan<'_>, policy: &PrivatePolicy<'_>) -> GuardrailDecision {
    if plan.validate_shape().is_err() {
        return GuardrailDecision::blocked(ExitCode::IntentSafety, ReasonCode::IntentSafety);
    }
    if policy.validate_shape().is_err() {
        return GuardrailDecision::blocked(ExitCode::ContractPolicy, ReasonCode::ContractPolicy);
    }

    if !contains_sorted(policy.allowed_contracts, plan.contract_id) {
        return GuardrailDecision::blocked(ExitCode::Allowlist, ReasonCode::Allowlist);
    }

    let contract_function = format!("{}:{}", plan.contract_id, plan.function);
    if !contains_sorted(
        policy.allowed_contract_functions,
        contract_function.as_str(),
    ) {
        return GuardrailDecision::blocked(ExitCode::ContractPolicy, ReasonCode::ContractPolicy);
    }

    if plan.intent_confidence_bps < policy.min_intent_confidence_bps {
        return GuardrailDecision::blocked(ExitCode::IntentSafety, ReasonCode::IntentSafety);
    }

    let Some(TypedValue::U64(amount_minor)) = find_arg(plan.args, "amount") else {
        return GuardrailDecision::blocked(ExitCode::IntentSafety, ReasonCode::IntentSafety);
    };
    let Some(TypedValue::Symbol(asset)) = find_arg(plan.args, "asset") else {
        return GuardrailDecision::blocked(ExitCode::IntentSafety, ReasonCode::IntentSafety);
    };
    let Some(TypedValue::Address(recipient)) = find_arg(plan.args, "recipient") else {
        return GuardrailDecision::blocked(ExitCode::IntentSafety, ReasonCode::IntentSafety);
    };

    if !contains_sorted(policy.allowed_assets, asset)
        || !contains_sorted(policy.allowed_recipients, recipient)
        || amount_minor > policy.max_amount_minor
    {
        return GuardrailDecision::blocked(ExitCode::ContractPolicy, ReasonCode::ContractPolicy);
    }

    if amount_minor >= policy.approval_threshold_minor {
        GuardrailDecision::requires_approval()
    } else {
        GuardrailDecision::approved()
    }
}

#[cfg(feature = "alloc")]
fn contains_sorted(values: &[&str], target: &str) -> bool {
    values.binary_search(&target).is_ok()
}

#[cfg(feature = "alloc")]
fn find_arg<'a>(args: &'a [TypedArg<'a>], name: &str) -> Option<TypedValue<'a>> {
    args.binary_search_by(|arg| arg.name.cmp(name))
        .ok()
        .map(|index| args[index].value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicJournal {
    pub contract_version: u32,
    pub evaluator_image_id: Digest32,
    pub action_plan_hash: Digest32,
    pub policy_commitment: Digest32,
    pub policy_version: u32,
    pub decision_status: DecisionStatus,
    pub exit_code: ExitCode,
    pub reason_code: ReasonCode,
    pub requires_approval: bool,
    pub audit_nullifier: Digest32,
}

impl PublicJournal {
    pub fn validate_semantics(&self) -> Result<(), ContractError> {
        if self.contract_version != CONTRACT_VERSION || self.policy_version == 0 {
            return Err(ContractError::UnsupportedVersion);
        }
        let valid = matches!(
            (
                self.decision_status,
                self.exit_code,
                self.reason_code,
                self.requires_approval,
            ),
            (
                DecisionStatus::Approved,
                ExitCode::Passed,
                ReasonCode::Passed,
                false,
            ) | (
                DecisionStatus::RequiresApproval,
                ExitCode::Passed,
                ReasonCode::ApprovalThreshold,
                true,
            ) | (
                DecisionStatus::Blocked,
                ExitCode::Allowlist,
                ReasonCode::Allowlist,
                false,
            ) | (
                DecisionStatus::Blocked,
                ExitCode::ContractPolicy,
                ReasonCode::ContractPolicy,
                false,
            ) | (
                DecisionStatus::Blocked,
                ExitCode::IntentSafety,
                ReasonCode::IntentSafety,
                false,
            )
        );
        if valid {
            Ok(())
        } else {
            Err(ContractError::InvalidJournalSemantics)
        }
    }

    #[cfg(feature = "alloc")]
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        self.validate_semantics()?;
        let mut out = Encoder::with_domain(PUBLIC_JOURNAL_DOMAIN);
        out.u32(self.contract_version);
        out.fixed32(&self.evaluator_image_id);
        out.fixed32(&self.action_plan_hash);
        out.fixed32(&self.policy_commitment);
        out.u32(self.policy_version);
        out.u8(self.decision_status as u8);
        out.u8(self.exit_code as u8);
        out.u8(self.reason_code as u8);
        out.u8(u8::from(self.requires_approval));
        out.fixed32(&self.audit_nullifier);
        Ok(out.finish())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ContractError> {
        let mut input = Decoder::new(bytes);
        input.domain(PUBLIC_JOURNAL_DOMAIN)?;
        let contract_version = input.u32()?;
        let evaluator_image_id = input.fixed32()?;
        let action_plan_hash = input.fixed32()?;
        let policy_commitment = input.fixed32()?;
        let policy_version = input.u32()?;
        let decision_status = DecisionStatus::from_tag(input.u8()?)?;
        let exit_code = ExitCode::from_tag(input.u8()?)?;
        let reason_code = ReasonCode::from_tag(input.u8()?)?;
        let requires_approval = match input.u8()? {
            0 => false,
            1 => true,
            _ => return Err(ContractError::InvalidEncoding),
        };
        let audit_nullifier = input.fixed32()?;
        input.finish()?;

        let journal = Self {
            contract_version,
            evaluator_image_id,
            action_plan_hash,
            policy_commitment,
            policy_version,
            decision_status,
            exit_code,
            reason_code,
            requires_approval,
            audit_nullifier,
        };
        journal.validate_semantics()?;
        Ok(journal)
    }
}

#[cfg(feature = "alloc")]
pub fn audit_nullifier_preimage(
    evaluator_image_id: &Digest32,
    action_plan_hash: &Digest32,
    policy_commitment: &Digest32,
    audit_nonce: &Digest32,
) -> Vec<u8> {
    let mut out = Encoder::with_domain(AUDIT_NULLIFIER_DOMAIN);
    out.fixed32(evaluator_image_id);
    out.fixed32(action_plan_hash);
    out.fixed32(policy_commitment);
    out.fixed32(audit_nonce);
    out.finish()
}

#[cfg(feature = "alloc")]
struct Encoder {
    bytes: Vec<u8>,
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn domain(&mut self, expected: &[u8]) -> Result<(), ContractError> {
        let actual = self.take(expected.len() + 1)?;
        if actual[..expected.len()] == *expected && actual[expected.len()] == 0 {
            Ok(())
        } else {
            Err(ContractError::InvalidEncoding)
        }
    }

    fn u8(&mut self) -> Result<u8, ContractError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, ContractError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| ContractError::InvalidEncoding)?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn fixed32(&mut self) -> Result<Digest32, ContractError> {
        self.take(32)?
            .try_into()
            .map_err(|_| ContractError::InvalidEncoding)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], ContractError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(ContractError::InvalidEncoding)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ContractError::InvalidEncoding)?;
        self.offset = end;
        Ok(value)
    }

    fn finish(self) -> Result<(), ContractError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ContractError::InvalidEncoding)
        }
    }
}

#[cfg(feature = "alloc")]
impl Encoder {
    fn with_domain(domain: &[u8]) -> Self {
        let mut bytes = Vec::with_capacity(256);
        bytes.extend_from_slice(domain);
        bytes.push(0);
        Self { bytes }
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.u32(value.len() as u32);
        self.bytes.extend_from_slice(value);
    }

    fn fixed32(&mut self, value: &Digest32) {
        self.bytes.extend_from_slice(value);
    }

    fn strings(&mut self, values: &[&str]) {
        self.u32(values.len() as u32);
        for value in values {
            self.string(value);
        }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT: &str = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    const OTHER_CONTRACT: &str = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    const RECIPIENT: &str = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    const ALLOWED_CONTRACTS: &[&str] = &[CONTRACT];
    const ALLOWED_CONTRACT_FUNCTIONS: &[&str] =
        &["CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ:purchase_credits"];
    const ALLOWED_ASSETS: &[&str] = &["USDC"];
    const ALLOWED_RECIPIENTS: &[&str] = &[RECIPIENT];

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn args() -> [TypedArg<'static>; 3] {
        [
            TypedArg {
                name: "amount",
                value: TypedValue::U64(500_000_000),
            },
            TypedArg {
                name: "asset",
                value: TypedValue::Symbol("USDC"),
            },
            TypedArg {
                name: "recipient",
                value: TypedValue::Address(RECIPIENT),
            },
        ]
    }

    fn plan<'a>(
        args: &'a [TypedArg<'a>],
        contract_id: &'a str,
        function: &'a str,
        intent_confidence_bps: u16,
    ) -> TypedActionPlan<'a> {
        TypedActionPlan {
            schema_version: CONTRACT_VERSION,
            intent_label: "ContractInvoke",
            action_kind: "soroban_contract_invoke",
            contract_id,
            function,
            args,
            intent_confidence_bps,
        }
    }

    fn policy(max_amount_minor: u64, approval_threshold_minor: u64) -> PrivatePolicy<'static> {
        PrivatePolicy {
            schema_version: CONTRACT_VERSION,
            policy_version: 1,
            commitment_salt: [0x55; 32],
            allowed_contracts: ALLOWED_CONTRACTS,
            allowed_contract_functions: ALLOWED_CONTRACT_FUNCTIONS,
            allowed_assets: ALLOWED_ASSETS,
            allowed_recipients: ALLOWED_RECIPIENTS,
            max_amount_minor,
            approval_threshold_minor,
            min_intent_confidence_bps: 9_000,
        }
    }

    fn journal(
        decision_status: DecisionStatus,
        exit_code: ExitCode,
        reason_code: ReasonCode,
        requires_approval: bool,
    ) -> PublicJournal {
        PublicJournal {
            contract_version: CONTRACT_VERSION,
            evaluator_image_id: [0x11; 32],
            action_plan_hash: [0x22; 32],
            policy_commitment: [0x33; 32],
            policy_version: 1,
            decision_status,
            exit_code,
            reason_code,
            requires_approval,
            audit_nullifier: [0x44; 32],
        }
    }

    #[test]
    fn action_plan_and_policy_preimages_are_domain_separated_and_stable() {
        let args = args();
        let plan = TypedActionPlan {
            schema_version: CONTRACT_VERSION,
            intent_label: "ContractInvoke",
            action_kind: "soroban_contract_invoke",
            contract_id: CONTRACT,
            function: "purchase_credits",
            args: &args,
            intent_confidence_bps: 9_800,
        };
        let policy = PrivatePolicy {
            schema_version: CONTRACT_VERSION,
            policy_version: 1,
            commitment_salt: [0x55; 32],
            allowed_contracts: &[CONTRACT],
            allowed_contract_functions: &[
                "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ:purchase_credits",
            ],
            allowed_assets: &["USDC"],
            allowed_recipients: &[RECIPIENT],
            max_amount_minor: 1_000_000_000,
            approval_threshold_minor: 600_000_000,
            min_intent_confidence_bps: 9_000,
        };

        let plan_bytes = plan.canonical_preimage().unwrap();
        let policy_bytes = policy.canonical_preimage().unwrap();
        assert!(plan_bytes.starts_with(ACTION_PLAN_DOMAIN));
        assert!(policy_bytes.starts_with(PRIVATE_POLICY_DOMAIN));
        assert_ne!(plan_bytes, policy_bytes);
        assert_eq!(plan_bytes, plan.canonical_preimage().unwrap());
        assert_eq!(policy_bytes, policy.canonical_preimage().unwrap());
        assert_eq!(
            to_hex(&plan_bytes),
            "4e435f5a4b5f414354494f4e5f504c414e5f563100000000010000000e436f6e7472616374496e766f6b6500000017736f726f62616e5f636f6e74726163745f696e766f6b650000003843444c464136464359484937524e334d4d54514a563554554b45594543514a41554537344844355a4a4d344e584d48434e344f4a4b43494a0000001070757263686173655f6372656469747326480000000300000006616d6f756e7404000000001dcd650000000005617373657403000000045553444300000009726563697069656e7401000000384743414c345049464b574f49464f365954345437545353455337534a43575637484e37584155544e46465347514b373452465553414a4258"
        );
        assert_eq!(
            to_hex(&policy_bytes),
            "4e435f5a4b5f505249564154455f504f4c4943595f56310000000001000000015555555555555555555555555555555555555555555555555555555555555555000000010000003843444c464136464359484937524e334d4d54514a563554554b45594543514a41554537344844355a4a4d344e584d48434e344f4a4b43494a000000010000004943444c464136464359484937524e334d4d54514a563554554b45594543514a41554537344844355a4a4d344e584d48434e344f4a4b43494a3a70757263686173655f6372656469747300000001000000045553444300000001000000384743414c345049464b574f49464f365954345437545353455337534a43575637484e37584155544e46465347514b373452465553414a4258000000003b9aca000000000023c346002328"
        );
    }

    #[test]
    fn canonical_order_is_enforced() {
        let args = [
            TypedArg {
                name: "recipient",
                value: TypedValue::Address(RECIPIENT),
            },
            TypedArg {
                name: "amount",
                value: TypedValue::U64(500_000_000),
            },
        ];
        let plan = TypedActionPlan {
            schema_version: CONTRACT_VERSION,
            intent_label: "ContractInvoke",
            action_kind: "soroban_contract_invoke",
            contract_id: CONTRACT,
            function: "purchase_credits",
            args: &args,
            intent_confidence_bps: 9_800,
        };
        assert_eq!(
            plan.canonical_preimage(),
            Err(ContractError::NonCanonicalOrder)
        );
    }

    #[test]
    fn journal_semantics_cover_pass_and_exit_3_4_5() {
        let cases = [
            journal(
                DecisionStatus::Approved,
                ExitCode::Passed,
                ReasonCode::Passed,
                false,
            ),
            journal(
                DecisionStatus::RequiresApproval,
                ExitCode::Passed,
                ReasonCode::ApprovalThreshold,
                true,
            ),
            journal(
                DecisionStatus::Blocked,
                ExitCode::Allowlist,
                ReasonCode::Allowlist,
                false,
            ),
            journal(
                DecisionStatus::Blocked,
                ExitCode::ContractPolicy,
                ReasonCode::ContractPolicy,
                false,
            ),
            journal(
                DecisionStatus::Blocked,
                ExitCode::IntentSafety,
                ReasonCode::IntentSafety,
                false,
            ),
        ];
        for case in cases {
            assert!(case.validate_semantics().is_ok());
            assert!(case.encode().is_ok());
        }

        let invalid = journal(
            DecisionStatus::Approved,
            ExitCode::ContractPolicy,
            ReasonCode::ContractPolicy,
            false,
        );
        assert_eq!(
            invalid.validate_semantics(),
            Err(ContractError::InvalidJournalSemantics)
        );
    }

    #[test]
    fn public_journal_strict_decode_roundtrips_and_rejects_tampering() {
        let journal = journal(
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
        );
        let encoded = journal.encode().unwrap();
        assert_eq!(encoded.len(), PUBLIC_JOURNAL_ENCODED_LEN);
        assert_eq!(PublicJournal::decode(&encoded), Ok(journal));

        let mut wrong_domain = encoded.clone();
        wrong_domain[0] ^= 1;
        assert_eq!(
            PublicJournal::decode(&wrong_domain),
            Err(ContractError::InvalidEncoding)
        );

        let tag_offset = PUBLIC_JOURNAL_DOMAIN.len() + 1 + 4 + (32 * 3) + 4;
        let mut invalid_tag = encoded.clone();
        invalid_tag[tag_offset] = 9;
        assert_eq!(
            PublicJournal::decode(&invalid_tag),
            Err(ContractError::InvalidEnumTag)
        );

        let mut semantic_mismatch = encoded.clone();
        semantic_mismatch[tag_offset + 1] = ExitCode::ContractPolicy as u8;
        semantic_mismatch[tag_offset + 2] = ReasonCode::ContractPolicy as u8;
        assert_eq!(
            PublicJournal::decode(&semantic_mismatch),
            Err(ContractError::InvalidJournalSemantics)
        );

        let mut trailing_data = encoded.clone();
        trailing_data.push(0);
        assert_eq!(
            PublicJournal::decode(&trailing_data),
            Err(ContractError::InvalidEncoding)
        );

        assert_eq!(
            PublicJournal::decode(&encoded[..encoded.len() - 1]),
            Err(ContractError::InvalidEncoding)
        );
    }

    #[test]
    fn evaluator_covers_approved_requires_approval_and_exit_3_4_5() {
        let args = args();
        let action = plan(&args, CONTRACT, "purchase_credits", 9_800);

        assert_eq!(
            evaluate(&action, &policy(1_000_000_000, 600_000_000)),
            GuardrailDecision::approved()
        );
        assert_eq!(
            evaluate(&action, &policy(1_000_000_000, 400_000_000)),
            GuardrailDecision::requires_approval()
        );

        let disallowed_contract = plan(&args, OTHER_CONTRACT, "purchase_credits", 9_800);
        assert_eq!(
            evaluate(&disallowed_contract, &policy(1_000_000_000, 600_000_000)),
            GuardrailDecision::blocked(ExitCode::Allowlist, ReasonCode::Allowlist)
        );

        assert_eq!(
            evaluate(&action, &policy(250_000_000, 200_000_000)),
            GuardrailDecision::blocked(ExitCode::ContractPolicy, ReasonCode::ContractPolicy)
        );

        let missing_recipient_args = &args[..2];
        let missing_recipient = plan(missing_recipient_args, CONTRACT, "purchase_credits", 9_800);
        assert_eq!(
            evaluate(&missing_recipient, &policy(1_000_000_000, 600_000_000)),
            GuardrailDecision::blocked(ExitCode::IntentSafety, ReasonCode::IntentSafety)
        );
    }

    #[test]
    fn evaluator_preserves_allowlist_then_policy_then_intent_order() {
        let args = args();
        let disallowed_and_low_confidence = plan(&args, OTHER_CONTRACT, "purchase_credits", 1_000);
        assert_eq!(
            evaluate(
                &disallowed_and_low_confidence,
                &policy(1_000_000_000, 600_000_000)
            ),
            GuardrailDecision::blocked(ExitCode::Allowlist, ReasonCode::Allowlist)
        );

        let disallowed_function_and_low_confidence =
            plan(&args, CONTRACT, "withdraw_everything", 1_000);
        assert_eq!(
            evaluate(
                &disallowed_function_and_low_confidence,
                &policy(1_000_000_000, 600_000_000)
            ),
            GuardrailDecision::blocked(ExitCode::ContractPolicy, ReasonCode::ContractPolicy)
        );

        let low_confidence = plan(&args, CONTRACT, "purchase_credits", 1_000);
        assert_eq!(
            evaluate(&low_confidence, &policy(1_000_000_000, 600_000_000)),
            GuardrailDecision::blocked(ExitCode::IntentSafety, ReasonCode::IntentSafety)
        );
    }

    #[test]
    fn evaluator_enforces_asset_recipient_and_typed_amount_policy() {
        let disallowed_asset_args = [
            TypedArg {
                name: "amount",
                value: TypedValue::U64(500_000_000),
            },
            TypedArg {
                name: "asset",
                value: TypedValue::Symbol("XLM"),
            },
            TypedArg {
                name: "recipient",
                value: TypedValue::Address(RECIPIENT),
            },
        ];
        let disallowed_asset = plan(&disallowed_asset_args, CONTRACT, "purchase_credits", 9_800);
        assert_eq!(
            evaluate(&disallowed_asset, &policy(1_000_000_000, 600_000_000)),
            GuardrailDecision::blocked(ExitCode::ContractPolicy, ReasonCode::ContractPolicy)
        );

        let disallowed_recipient_args = [
            TypedArg {
                name: "amount",
                value: TypedValue::U64(500_000_000),
            },
            TypedArg {
                name: "asset",
                value: TypedValue::Symbol("USDC"),
            },
            TypedArg {
                name: "recipient",
                value: TypedValue::Address("GDISALLOWEDRECIPIENT"),
            },
        ];
        let disallowed_recipient = plan(
            &disallowed_recipient_args,
            CONTRACT,
            "purchase_credits",
            9_800,
        );
        assert_eq!(
            evaluate(&disallowed_recipient, &policy(1_000_000_000, 600_000_000)),
            GuardrailDecision::blocked(ExitCode::ContractPolicy, ReasonCode::ContractPolicy)
        );

        let wrongly_typed_amount_args = [
            TypedArg {
                name: "amount",
                value: TypedValue::Symbol("500000000"),
            },
            TypedArg {
                name: "asset",
                value: TypedValue::Symbol("USDC"),
            },
            TypedArg {
                name: "recipient",
                value: TypedValue::Address(RECIPIENT),
            },
        ];
        let wrongly_typed_amount = plan(
            &wrongly_typed_amount_args,
            CONTRACT,
            "purchase_credits",
            9_800,
        );
        assert_eq!(
            evaluate(&wrongly_typed_amount, &policy(1_000_000_000, 600_000_000)),
            GuardrailDecision::blocked(ExitCode::IntentSafety, ReasonCode::IntentSafety)
        );
    }

    #[test]
    fn evaluator_decision_projects_to_valid_public_journal() {
        let args = args();
        let action = plan(&args, CONTRACT, "purchase_credits", 9_800);
        let decision = evaluate(&action, &policy(1_000_000_000, 400_000_000));
        let journal = decision.into_journal([1; 32], [2; 32], [3; 32], 1, [4; 32]);
        assert!(journal.validate_semantics().is_ok());
        assert_eq!(journal.decision_status, DecisionStatus::RequiresApproval);
        assert!(journal.requires_approval);
    }

    #[test]
    fn audit_nullifier_binds_image_plan_policy_and_nonce() {
        let base = audit_nullifier_preimage(&[1; 32], &[2; 32], &[3; 32], &[4; 32]);
        assert!(base.starts_with(AUDIT_NULLIFIER_DOMAIN));
        assert_ne!(
            base,
            audit_nullifier_preimage(&[1; 32], &[2; 32], &[3; 32], &[5; 32])
        );
    }
}
