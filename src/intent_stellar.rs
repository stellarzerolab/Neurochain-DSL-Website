use std::env;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::actions::{Action, ActionPlan};
use crate::ai::model::AIModel;

pub const DEFAULT_INTENT_STELLAR_THRESHOLD: f32 = 0.55;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentStellarLabel {
    BalanceQuery,
    CreateAccount,
    ChangeTrust,
    TransferXLM,
    TransferAsset,
    FundTestnet,
    TxStatus,
    ContractInvoke,
    Unknown,
}

impl IntentStellarLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            IntentStellarLabel::BalanceQuery => "BalanceQuery",
            IntentStellarLabel::CreateAccount => "CreateAccount",
            IntentStellarLabel::ChangeTrust => "ChangeTrust",
            IntentStellarLabel::TransferXLM => "TransferXLM",
            IntentStellarLabel::TransferAsset => "TransferAsset",
            IntentStellarLabel::FundTestnet => "FundTestnet",
            IntentStellarLabel::TxStatus => "TxStatus",
            IntentStellarLabel::ContractInvoke => "ContractInvoke",
            IntentStellarLabel::Unknown => "Unknown",
        }
    }

    pub fn from_label(raw: &str) -> Self {
        match raw.trim() {
            "BalanceQuery" => IntentStellarLabel::BalanceQuery,
            "CreateAccount" => IntentStellarLabel::CreateAccount,
            "ChangeTrust" => IntentStellarLabel::ChangeTrust,
            "TransferXLM" => IntentStellarLabel::TransferXLM,
            "TransferAsset" => IntentStellarLabel::TransferAsset,
            "FundTestnet" => IntentStellarLabel::FundTestnet,
            "TxStatus" => IntentStellarLabel::TxStatus,
            "ContractInvoke" => IntentStellarLabel::ContractInvoke,
            _ => IntentStellarLabel::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntentDecision {
    pub label: IntentStellarLabel,
    pub score: f32,
    pub threshold: f32,
    pub downgraded_to_unknown: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct IntentBuildConfig {
    pub threshold: f32,
}

impl Default for IntentBuildConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_INTENT_STELLAR_THRESHOLD,
        }
    }
}

impl IntentBuildConfig {
    pub fn from_env() -> Result<Self> {
        let threshold = threshold_from_env()?.unwrap_or(DEFAULT_INTENT_STELLAR_THRESHOLD);
        Ok(Self { threshold })
    }
}

pub fn resolve_model_path() -> String {
    if let Ok(path) = env::var("NC_INTENT_STELLAR_MODEL") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let base = env::var("NC_MODELS_DIR").unwrap_or_else(|_| "models".to_string());
    format!("{base}/intent_stellar/model.onnx")
}

pub fn threshold_from_env() -> Result<Option<f32>> {
    let Some(raw) = env::var("NC_INTENT_STELLAR_THRESHOLD").ok() else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed
        .parse::<f32>()
        .with_context(|| format!("invalid NC_INTENT_STELLAR_THRESHOLD: {trimmed}"))?;
    Ok(Some(parsed))
}

fn decide_label(raw_label: &str, score: f32, threshold: f32) -> IntentDecision {
    let original = IntentStellarLabel::from_label(raw_label);
    let downgraded_to_unknown = original != IntentStellarLabel::Unknown && score < threshold;
    let label = if downgraded_to_unknown {
        IntentStellarLabel::Unknown
    } else {
        original
    };
    IntentDecision {
        label,
        score,
        threshold,
        downgraded_to_unknown,
    }
}

pub fn classify(prompt: &str, model_path: &str, threshold: f32) -> Result<IntentDecision> {
    let model = AIModel::new(model_path)
        .with_context(|| format!("failed to load intent_stellar model from {model_path}"))?;
    let (raw_label, score) = model
        .predict_with_score(prompt)
        .context("intent_stellar classification failed")?;
    Ok(decide_label(&raw_label, score, threshold))
}

pub fn build_action_plan(prompt: &str, decision: &IntentDecision) -> ActionPlan {
    let mut plan = ActionPlan {
        source: Some("intent_stellar".to_string()),
        ..ActionPlan::default()
    };
    plan.warnings.push(format!(
        "intent_info: label={} score={:.4} threshold={:.2}",
        decision.label.as_str(),
        decision.score,
        decision.threshold
    ));

    if decision.downgraded_to_unknown {
        plan.warnings.push(format!(
            "intent_warning: low_confidence score={:.4} threshold={:.2}",
            decision.score, decision.threshold
        ));
        plan.actions.push(Action::Unknown {
            reason: format!(
                "intent_low_confidence: score={:.4} threshold={:.2}",
                decision.score, decision.threshold
            ),
        });
        return plan;
    }

    let result = match decision.label {
        IntentStellarLabel::BalanceQuery => build_balance_query(prompt),
        IntentStellarLabel::CreateAccount => build_create_account(prompt),
        IntentStellarLabel::ChangeTrust => build_change_trust(prompt),
        IntentStellarLabel::TransferXLM => build_transfer_xlm(prompt),
        IntentStellarLabel::TransferAsset => build_transfer_asset(prompt),
        IntentStellarLabel::FundTestnet => build_fund_testnet(prompt),
        IntentStellarLabel::TxStatus => build_tx_status(prompt),
        IntentStellarLabel::ContractInvoke => build_contract_invoke(prompt),
        IntentStellarLabel::Unknown => {
            Err("slot_missing: Unknown intent has no action mapping".to_string())
        }
    };

    match result {
        Ok(action) => plan.actions.push(action),
        Err(err) => {
            plan.warnings.push(format!("intent_error: {err}"));
            plan.actions.push(Action::Unknown { reason: err });
        }
    }

    plan
}

fn account_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bG[A-Z2-7]{55}\b").expect("account regex"))
}

fn contract_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bC[A-Z2-7]{55}\b").expect("contract regex"))
}

fn tx_hash_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[a-fA-F0-9]{64}\b").expect("tx hash regex"))
}

fn amount_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d+(?:\.\d+)?\b").expect("amount regex"))
}

fn asset_pair_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([A-Z0-9]{1,12}):(G[A-Z2-7]{55})\b").expect("asset regex"))
}

fn function_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\bfunction\s+([A-Za-z_][A-Za-z0-9_]{0,31})\b").expect("function regex")
    })
}

fn destination_account_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:to|destination|recipient)\b.*?(G[A-Z2-7]{55})")
            .expect("destination regex")
    })
}

fn extract_first_account(prompt: &str) -> Option<String> {
    account_re().find(prompt).map(|m| m.as_str().to_string())
}

fn extract_nth_account(prompt: &str, index: usize) -> Option<String> {
    if index == 0 {
        return None;
    }
    account_re()
        .find_iter(prompt)
        .nth(index - 1)
        .map(|m| m.as_str().to_string())
}

fn extract_first_contract(prompt: &str) -> Option<String> {
    contract_re().find(prompt).map(|m| m.as_str().to_string())
}

fn extract_first_hash(prompt: &str) -> Option<String> {
    tx_hash_re().find(prompt).map(|m| m.as_str().to_string())
}

fn extract_first_amount(prompt: &str) -> Option<String> {
    amount_re().find(prompt).map(|m| m.as_str().to_string())
}

fn extract_asset_pair(prompt: &str) -> Option<(String, String)> {
    let captures = asset_pair_re().captures(prompt)?;
    let code = captures.get(1)?.as_str().to_string();
    let issuer = captures.get(2)?.as_str().to_string();
    Some((code, issuer))
}

fn extract_balance_asset(prompt: &str) -> Option<String> {
    if let Some((code, issuer)) = extract_asset_pair(prompt) {
        return Some(format!("{code}:{issuer}"));
    }
    if prompt.to_ascii_lowercase().contains("xlm") {
        return Some("XLM".to_string());
    }
    None
}

fn extract_function(prompt: &str) -> Option<String> {
    let captures = function_re().captures(prompt)?;
    Some(captures.get(1)?.as_str().to_string())
}

fn extract_destination_account(prompt: &str) -> Option<String> {
    destination_account_re()
        .captures(prompt)
        .and_then(|captures| captures.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_json_block(src: &str) -> Option<(String, usize)> {
    let mut chars = src.char_indices();
    let (mut end_idx, opener) = chars.next()?;
    let closer = match opener {
        '{' => '}',
        '[' => ']',
        _ => return None,
    };

    let mut depth: usize = 1;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in chars {
        end_idx = idx;
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            continue;
        }
        if ch == opener {
            depth += 1;
            continue;
        }
        if ch == closer {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                let consumed = idx + ch.len_utf8();
                return Some((src[..consumed].to_string(), consumed));
            }
        }
    }

    if depth == 0 {
        let consumed = end_idx + opener.len_utf8();
        return Some((src[..consumed].to_string(), consumed));
    }
    None
}

fn extract_named_json(prompt: &str, key: &str) -> Option<Value> {
    let lower = prompt.to_ascii_lowercase();
    let idx = lower.find(key)?;
    let tail = prompt[idx + key.len()..].trim_start();
    let (json_text, _) = extract_json_block(tail)?;
    serde_json::from_str::<Value>(&json_text).ok()
}

fn extract_args_json(prompt: &str) -> Value {
    if let Some(value) = extract_named_json(prompt, "args=") {
        return value;
    }

    if let Some(pos) = prompt.find('{') {
        let tail = &prompt[pos..];
        if let Some((json_text, consumed)) = extract_json_block(tail) {
            let rest = tail[consumed..].trim();
            if rest.is_empty() {
                if let Ok(value) = serde_json::from_str::<Value>(&json_text) {
                    return value;
                }
            }
        }
    }

    Value::Object(serde_json::Map::new())
}

fn extract_arg_types(prompt: &str) -> Result<Option<serde_json::Map<String, Value>>, String> {
    let arg_types =
        extract_named_json(prompt, "arg_types=").or_else(|| extract_named_json(prompt, "types="));
    let Some(arg_types) = arg_types else {
        return Ok(None);
    };
    let Some(obj) = arg_types.as_object() else {
        return Err("slot_type_error: ContractInvoke arg_types must be object JSON".to_string());
    };
    Ok(Some(obj.clone()))
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

fn value_matches_typed_slot(value: &Value, ty: &str) -> bool {
    match ty {
        "address" => value.as_str().map(is_strkey).unwrap_or(false),
        "bytes" => value.as_str().map(is_hex_bytes).unwrap_or(false),
        "symbol" => value.as_str().map(is_symbol).unwrap_or(false),
        "u64" => is_u64_value(value),
        _ => false,
    }
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
                let changed = value != &Value::Number(parsed.into());
                *value = Value::Number(parsed.into());
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

fn validate_contract_invoke_arg_types(
    args: &mut Value,
    arg_types: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    let Some(args_obj) = args.as_object() else {
        return Err("slot_missing: ContractInvoke args must be object JSON".to_string());
    };
    let mut errors: Vec<String> = Vec::new();
    let mut normalized_updates: Vec<(String, Value)> = Vec::new();

    for (key, value) in arg_types {
        let Some(ty_raw) = value.as_str() else {
            errors.push(format!(
                "slot_type_error: ContractInvoke arg_types.{key} must be string"
            ));
            continue;
        };
        let ty = ty_raw.trim().to_ascii_lowercase();
        if !matches!(ty.as_str(), "address" | "bytes" | "symbol" | "u64") {
            errors.push(format!(
                "slot_type_error: ContractInvoke arg_types.{key} unsupported type {ty_raw}"
            ));
            continue;
        }
        let Some(arg_value) = args_obj.get(key) else {
            errors.push(format!(
                "slot_type_error: ContractInvoke missing typed arg {key}:{ty}"
            ));
            continue;
        };
        let mut normalized_value = arg_value.clone();
        if let Err(detail) = normalize_typed_slot_value(&mut normalized_value, ty.as_str()) {
            errors.push(format!("slot_type_error: ContractInvoke {key} {detail}"));
            continue;
        }
        if !value_matches_typed_slot(&normalized_value, ty.as_str()) {
            errors.push(format!(
                "slot_type_error: ContractInvoke {key} expected {ty}"
            ));
            continue;
        }
        if &normalized_value != arg_value {
            normalized_updates.push((key.clone(), normalized_value));
        }
    }

    if !errors.is_empty() {
        return Err(errors.join("; "));
    }

    let Some(args_obj_mut) = args.as_object_mut() else {
        return Err("slot_missing: ContractInvoke args must be object JSON".to_string());
    };
    for (key, value) in normalized_updates {
        args_obj_mut.insert(key, value);
    }

    Ok(())
}

fn slot_missing(intent: IntentStellarLabel, slot: &str) -> String {
    format!("slot_missing: {} missing {slot}", intent.as_str())
}

fn require_account(
    prompt: &str,
    intent: IntentStellarLabel,
    slot_name: &str,
) -> Result<String, String> {
    extract_first_account(prompt).ok_or_else(|| slot_missing(intent, slot_name))
}

fn require_amount(
    prompt: &str,
    intent: IntentStellarLabel,
    slot_name: &str,
) -> Result<String, String> {
    extract_first_amount(prompt).ok_or_else(|| slot_missing(intent, slot_name))
}

fn require_asset_pair(
    prompt: &str,
    intent: IntentStellarLabel,
) -> Result<(String, String), String> {
    extract_asset_pair(prompt).ok_or_else(|| slot_missing(intent, "asset_code/asset_issuer"))
}

fn build_balance_query(prompt: &str) -> Result<Action, String> {
    let account = require_account(prompt, IntentStellarLabel::BalanceQuery, "account")?;
    Ok(Action::StellarAccountBalance {
        account,
        asset: extract_balance_asset(prompt),
    })
}

fn build_create_account(prompt: &str) -> Result<Action, String> {
    let destination = require_account(prompt, IntentStellarLabel::CreateAccount, "destination")?;
    let starting_balance = require_amount(
        prompt,
        IntentStellarLabel::CreateAccount,
        "starting_balance",
    )?;
    Ok(Action::StellarAccountCreate {
        destination,
        starting_balance,
    })
}

fn build_change_trust(prompt: &str) -> Result<Action, String> {
    let (asset_code, asset_issuer) = require_asset_pair(prompt, IntentStellarLabel::ChangeTrust)?;
    Ok(Action::StellarChangeTrust {
        asset_code,
        asset_issuer,
        limit: extract_first_amount(prompt),
    })
}

fn build_transfer_xlm(prompt: &str) -> Result<Action, String> {
    let to = extract_destination_account(prompt)
        .or_else(|| extract_first_account(prompt))
        .ok_or_else(|| slot_missing(IntentStellarLabel::TransferXLM, "to"))?;
    let amount = require_amount(prompt, IntentStellarLabel::TransferXLM, "amount")?;
    Ok(Action::StellarPayment {
        to,
        amount,
        asset_code: "XLM".to_string(),
        asset_issuer: None,
    })
}

fn build_transfer_asset(prompt: &str) -> Result<Action, String> {
    let to = extract_destination_account(prompt)
        .or_else(|| extract_nth_account(prompt, 2))
        .or_else(|| extract_first_account(prompt))
        .ok_or_else(|| slot_missing(IntentStellarLabel::TransferAsset, "to"))?;
    let amount = require_amount(prompt, IntentStellarLabel::TransferAsset, "amount")?;
    let (asset_code, asset_issuer) = require_asset_pair(prompt, IntentStellarLabel::TransferAsset)?;
    Ok(Action::StellarPayment {
        to,
        amount,
        asset_code,
        asset_issuer: Some(asset_issuer),
    })
}

fn build_fund_testnet(prompt: &str) -> Result<Action, String> {
    let account = require_account(prompt, IntentStellarLabel::FundTestnet, "account")?;
    Ok(Action::StellarAccountFundTestnet { account })
}

fn build_tx_status(prompt: &str) -> Result<Action, String> {
    let hash = extract_first_hash(prompt)
        .ok_or_else(|| slot_missing(IntentStellarLabel::TxStatus, "hash"))?;
    Ok(Action::StellarTxStatus { hash })
}

fn build_contract_invoke(prompt: &str) -> Result<Action, String> {
    let contract_id = extract_first_contract(prompt)
        .ok_or_else(|| slot_missing(IntentStellarLabel::ContractInvoke, "contract_id"))?;
    let function = extract_function(prompt)
        .ok_or_else(|| slot_missing(IntentStellarLabel::ContractInvoke, "function"))?;
    let mut args = extract_args_json(prompt);
    if !args.is_object() {
        return Err("slot_missing: ContractInvoke args must be object JSON".to_string());
    }
    if let Some(arg_types) = extract_arg_types(prompt)? {
        validate_contract_invoke_arg_types(&mut args, &arg_types)?;
    }
    Ok(Action::SorobanContractInvoke {
        contract_id,
        function,
        args,
    })
}

pub fn has_intent_blocking_issue(plan: &ActionPlan) -> bool {
    let has_unknown = plan
        .actions
        .iter()
        .any(|action| matches!(action, Action::Unknown { .. }));
    if has_unknown {
        return true;
    }
    plan.warnings.iter().any(|warning| {
        warning.starts_with("intent_error:") || warning.starts_with("intent_warning:")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(label: IntentStellarLabel) -> IntentDecision {
        IntentDecision {
            label,
            score: 0.91,
            threshold: DEFAULT_INTENT_STELLAR_THRESHOLD,
            downgraded_to_unknown: false,
        }
    }

    #[test]
    fn low_confidence_downgrades_to_unknown() {
        let d = decide_label("TransferXLM", 0.20, DEFAULT_INTENT_STELLAR_THRESHOLD);
        assert_eq!(d.label, IntentStellarLabel::Unknown);
        assert!(d.downgraded_to_unknown);
    }

    #[test]
    fn build_action_for_each_label() {
        let g1 = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
        let g2 = "GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ";
        let c1 = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
        let hash = "f3eb378466903fc8eb132f67bc33519bb1233f5f78df4d9f0f6998a1445e5f15";

        let cases = vec![
            (
                IntentStellarLabel::BalanceQuery,
                format!("Check balance for {g1} asset XLM"),
                "stellar.account.balance",
            ),
            (
                IntentStellarLabel::CreateAccount,
                format!("Create account {g2} with 2 XLM"),
                "stellar.account.create",
            ),
            (
                IntentStellarLabel::ChangeTrust,
                format!("Add trustline TESTUSD:{g1} limit 1000"),
                "stellar.change_trust",
            ),
            (
                IntentStellarLabel::TransferXLM,
                format!("Send 5 XLM to {g2}"),
                "stellar.payment",
            ),
            (
                IntentStellarLabel::TransferAsset,
                format!("Send 12.5 TESTUSD:{g1} to {g2}"),
                "stellar.payment",
            ),
            (
                IntentStellarLabel::FundTestnet,
                format!("Fund testnet account {g1}"),
                "stellar.account.fund_testnet",
            ),
            (
                IntentStellarLabel::TxStatus,
                format!("Check tx status {hash}"),
                "stellar.tx.status",
            ),
            (
                IntentStellarLabel::ContractInvoke,
                format!("Invoke contract {c1} function hello args={{\"to\":\"world\"}}"),
                "soroban.contract.invoke",
            ),
        ];

        for (label, prompt, expected_kind) in cases {
            let plan = build_action_plan(&prompt, &decision(label));
            assert_eq!(plan.actions.len(), 1);
            assert_eq!(plan.actions[0].kind(), expected_kind);
        }
    }

    #[test]
    fn slot_missing_creates_unknown_for_critical_labels() {
        let transfer = build_action_plan(
            "send xlm please",
            &decision(IntentStellarLabel::TransferXLM),
        );
        let trust = build_action_plan(
            "add trustline TESTUSD only",
            &decision(IntentStellarLabel::ChangeTrust),
        );
        let invoke = build_action_plan(
            "invoke contract now",
            &decision(IntentStellarLabel::ContractInvoke),
        );

        for plan in [transfer, trust, invoke] {
            assert!(matches!(plan.actions[0], Action::Unknown { .. }));
            assert!(plan
                .warnings
                .iter()
                .any(|w| w.starts_with("intent_error: slot_missing")));
        }
    }

    #[test]
    fn contract_invoke_typed_slots_accept_valid_payload() {
        let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
        let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
        let prompt = format!(
            "Invoke contract {contract} function transfer args={{\"to\":\"{account}\",\"blob\":\"0x0A0B\",\"ticker\":\"USDC\",\"amount\":100}} arg_types={{\"to\":\"address\",\"blob\":\"bytes\",\"ticker\":\"symbol\",\"amount\":\"u64\"}}"
        );
        let plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].kind(), "soroban.contract.invoke");
        assert!(plan
            .warnings
            .iter()
            .all(|w| !w.starts_with("intent_error: slot_type_error")));
    }

    #[test]
    fn contract_invoke_typed_slots_normalize_valid_payload() {
        let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
        let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
        let prompt = format!(
            "Invoke contract {contract} function transfer args={{\"to\":\"{}\",\"blob\":\"0X0A0B\",\"ticker\":\" USDC \",\"amount\":\"00100\"}} arg_types={{\"to\":\"address\",\"blob\":\"bytes\",\"ticker\":\"symbol\",\"amount\":\"u64\"}}",
            account.to_ascii_lowercase()
        );
        let plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
        assert_eq!(plan.actions.len(), 1);
        match &plan.actions[0] {
            Action::SorobanContractInvoke { args, .. } => {
                assert_eq!(args["to"].as_str(), Some(account));
                assert_eq!(args["blob"].as_str(), Some("0x0a0b"));
                assert_eq!(args["ticker"].as_str(), Some("USDC"));
                assert_eq!(args["amount"].as_u64(), Some(100));
            }
            other => panic!("expected SorobanContractInvoke, got {other:?}"),
        }
    }

    #[test]
    fn contract_invoke_typed_slots_normalize_practical_separator_payload() {
        let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
        let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
        let prompt = format!(
            "Invoke contract {contract} function transfer args={{\"to\":\" {} \",\"blob\":\"0XDE AD_be-EF\",\"ticker\":\" USDC \",\"amount\":\"1_000,000\"}} arg_types={{\"to\":\"address\",\"blob\":\"bytes\",\"ticker\":\"symbol\",\"amount\":\"u64\"}}",
            account.to_ascii_lowercase()
        );
        let plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
        assert_eq!(plan.actions.len(), 1);
        match &plan.actions[0] {
            Action::SorobanContractInvoke { args, .. } => {
                assert_eq!(args["to"].as_str(), Some(account));
                assert_eq!(args["blob"].as_str(), Some("0xdeadbeef"));
                assert_eq!(args["ticker"].as_str(), Some("USDC"));
                assert_eq!(args["amount"].as_u64(), Some(1_000_000));
            }
            other => panic!("expected SorobanContractInvoke, got {other:?}"),
        }
    }

    #[test]
    fn contract_invoke_typed_slots_type_error_creates_unknown() {
        let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
        let prompt = format!(
            "Invoke contract {contract} function transfer args={{\"to\":\"World\",\"amount\":-1}} arg_types={{\"to\":\"address\",\"amount\":\"u64\"}}"
        );
        let plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
        assert!(matches!(plan.actions[0], Action::Unknown { .. }));
        assert!(plan
            .warnings
            .iter()
            .any(|w| w.starts_with("intent_error: slot_type_error")));
    }

    #[test]
    fn contract_invoke_typed_slots_report_multiple_errors() {
        let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
        let prompt = format!(
            "Invoke contract {contract} function transfer args={{\"to\":\"World\",\"blob\":\"XYZ\",\"amount\":-1}} arg_types={{\"to\":\"address\",\"blob\":\"bytes\",\"amount\":\"u64\"}}"
        );
        let plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
        assert!(matches!(plan.actions[0], Action::Unknown { .. }));
        let joined = plan.warnings.join(" | ");
        assert!(joined.contains("ContractInvoke to"));
        assert!(joined.contains("expected address"));
        assert!(joined.contains("ContractInvoke blob"));
        assert!(joined.contains("expected bytes"));
        assert!(joined.contains("ContractInvoke amount"));
        assert!(joined.contains("expected u64"));
    }

    #[test]
    fn contract_invoke_typed_slots_report_stage3_edge_errors() {
        let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
        let prompt = format!(
            "Invoke contract {contract} function transfer args={{\"to\":\"World\",\"blob\":\"0xABC\",\"ticker\":\" BAD VALUE \",\"amount\":\"18446744073709551616\"}} arg_types={{\"to\":\"address\",\"blob\":\"bytes\",\"ticker\":\"symbol\",\"amount\":\"u64\"}}"
        );
        let plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
        assert!(matches!(plan.actions[0], Action::Unknown { .. }));
        let joined = plan.warnings.join(" | ");
        assert!(joined.contains("ContractInvoke to"));
        assert!(joined.contains("ContractInvoke blob"));
        assert!(joined.contains("ContractInvoke ticker"));
        assert!(joined.contains("ContractInvoke amount"));
        assert!(joined.contains("expected address"));
        assert!(joined.contains("expected bytes"));
        assert!(joined.contains("expected symbol"));
        assert!(joined.contains("expected u64"));
    }

    #[test]
    fn transfer_asset_prefers_destination_account_over_issuer() {
        let issuer = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
        let recipient = "GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ";
        let prompt = format!("Send 12.5 TESTUSD:{issuer} to {recipient}");

        let plan = build_action_plan(&prompt, &decision(IntentStellarLabel::TransferAsset));
        assert_eq!(plan.actions.len(), 1);
        match &plan.actions[0] {
            Action::StellarPayment { to, .. } => assert_eq!(to, recipient),
            other => panic!("expected StellarPayment, got {other:?}"),
        }
    }
}
