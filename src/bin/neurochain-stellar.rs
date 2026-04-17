use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use neurochain::actions::{
    parse_action_plan_from_nc, validate_plan, Action, ActionPlan, Allowlist,
};
use neurochain::ai::model::AIModel;
use neurochain::banner;
use neurochain::help_text::neurochain_language_help;
use neurochain::intent_stellar::{
    build_action_plan as build_intent_action_plan, classify as classify_intent_stellar,
    has_intent_blocking_issue, resolve_model_path as resolve_intent_model_path,
    threshold_from_env as intent_threshold_from_env, DEFAULT_INTENT_STELLAR_THRESHOLD,
};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

fn print_usage() {
    eprintln!(
        "Usage: neurochain-stellar [<file.nc|plan.json>] [--flow|--no-flow] [--yes] [--debug] [--intent-text \"...\"] [--intent-model <path>] [--intent-threshold <f32>]"
    );
    eprintln!("Usage: neurochain-stellar --repl [--flow|--no-flow] [--debug]");
    eprintln!("If no args are provided, REPL mode is started (flow enabled by default).");
    eprintln!("If input is JSON, it is treated as an ActionPlan.");
    eprintln!(
        "Manual .nc lines can start with 'stellar.' or 'soroban.' (comment lines are ignored)."
    );
    eprintln!(
        ".nc files also support: AI/network/wallet + txrep/x402/horizon/friendbot/stellar_cli/simulate_flag/intent_threshold."
    );
    eprintln!("Run `help` in REPL to see all in-CLI/.nc config commands (env equivalents).");
    eprintln!("--intent-text enables IntentStellar -> ActionPlan mode.");
    eprintln!("--intent-model overrides the intent_stellar model path.");
    eprintln!("--intent-threshold overrides confidence threshold (default: 0.55).");
    eprintln!("--flow enables simulate -> preview -> confirm -> submit.");
    eprintln!("--no-flow forces plan-only mode (no preview/submit).");
    eprintln!("--yes auto-confirms submit in --flow mode.");
    eprintln!(
        "--debug enables intent pipeline trace (classify -> slot-parse -> guardrails -> flow)."
    );
    eprintln!("Flow in intent mode is blocked when plan has Unknown/intent_error (exit code 5).");
    eprintln!("Env: NC_STELLAR_NETWORK / NC_SOROBAN_NETWORK (default: testnet)");
    eprintln!("Env: NC_STELLAR_HORIZON_URL (default: testnet Horizon)");
    eprintln!("Env: NC_FRIENDBOT_URL (default: testnet Friendbot)");
    eprintln!("Env: NC_SOROBAN_SOURCE or NC_STELLAR_SOURCE (for soroban invoke)");
    eprintln!("Env: NC_STELLAR_CLI (default: stellar)");
    eprintln!("Env: NC_SOROBAN_SIMULATE_FLAG (default: \"--send no\")");
    eprintln!("Env: NC_TXREP_PREVIEW=1 (include txrep in preview output)");
    eprintln!("Env: NC_INTENT_STELLAR_MODEL (default: models/intent_stellar/model.onnx)");
    eprintln!("Env: NC_INTENT_STELLAR_THRESHOLD (default: 0.55)");
    eprintln!("Env: NC_INTENT_DEBUG=1 (enable intent pipeline trace)");
    eprintln!("Env: NC_ASSET_ALLOWLIST (e.g. XLM,USDC:GISSUER)");
    eprintln!("Env: NC_SOROBAN_ALLOWLIST (e.g. C1:transfer,C2)");
    eprintln!("Env: NC_ALLOWLIST_ENFORCE=1 (hard-fail on allowlist violations)");
    eprintln!("Env: NC_CONTRACT_POLICY=path/to/policy.json");
    eprintln!("Env: NC_CONTRACT_POLICY_DIR=contracts");
    eprintln!("Env: NC_CONTRACT_POLICY_ENFORCE=1 (hard-fail on policy violations)");
    eprintln!("Docs: docs/stellar_actions_guide.md (see section 2.1 Env-matrix)");
}

#[derive(Debug, Default)]
struct CliArgs {
    repl: bool,
    path: Option<String>,
    flow: bool,
    debug: bool,
    auto_yes: bool,
    intent_text: Option<String>,
    intent_model: Option<String>,
    intent_threshold: Option<f32>,
}

fn parse_cli_args(args: &[String]) -> Result<CliArgs> {
    let mut out = CliArgs::default();
    if args.len() <= 1 {
        out.repl = true;
        out.flow = true;
        return Ok(out);
    }

    let mut flow_explicit = false;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--repl" => out.repl = true,
            "--flow" => {
                out.flow = true;
                flow_explicit = true;
            }
            "--no-flow" => {
                out.flow = false;
                flow_explicit = true;
            }
            "--debug" => out.debug = true,
            "--yes" | "-y" => out.auto_yes = true,
            "--intent-text" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| anyhow!("missing value for --intent-text"))?;
                out.intent_text = Some(value.clone());
            }
            "--intent-model" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| anyhow!("missing value for --intent-model"))?;
                out.intent_model = Some(value.clone());
            }
            "--intent-threshold" => {
                i += 1;
                let raw = args
                    .get(i)
                    .ok_or_else(|| anyhow!("missing value for --intent-threshold"))?;
                let value = raw
                    .parse::<f32>()
                    .with_context(|| format!("invalid --intent-threshold value: {raw}"))?;
                out.intent_threshold = Some(value);
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                return Err(anyhow!("unknown flag: {other}"));
            }
            other => {
                if out.path.is_none() {
                    out.path = Some(other.to_string());
                } else {
                    return Err(anyhow!("multiple input paths are not supported"));
                }
            }
        }
        i += 1;
    }

    if out.path.is_some() && out.intent_text.is_some() {
        return Err(anyhow!("use either <file> or --intent-text, not both"));
    }
    if out.path.is_none() && out.intent_text.is_none() {
        out.repl = true;
    }

    if out.repl {
        if out.path.is_some() || out.intent_text.is_some() {
            return Err(anyhow!(
                "--repl cannot be combined with <file> or --intent-text"
            ));
        }
        if !flow_explicit {
            out.flow = true;
        }
        return Ok(out);
    }

    Ok(out)
}

fn parse_bool_value(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn intent_debug_from_env() -> bool {
    parse_bool_value(&env::var("NC_INTENT_DEBUG").unwrap_or_default()).unwrap_or(false)
}

fn resolve_intent_debug(cli_debug: bool) -> bool {
    cli_debug || intent_debug_from_env()
}

fn intent_debug_log(enabled: bool, stage: &str, message: impl AsRef<str>) {
    if enabled {
        eprintln!("[intent-debug] {stage}: {}", message.as_ref());
    }
}

fn allowlist_enforced(override_value: Option<bool>) -> bool {
    if let Some(value) = override_value {
        return value;
    }
    parse_bool_value(&std::env::var("NC_ALLOWLIST_ENFORCE").unwrap_or_default()).unwrap_or(false)
}

fn policy_enforced(override_value: Option<bool>) -> bool {
    if let Some(value) = override_value {
        return value;
    }
    parse_bool_value(&std::env::var("NC_CONTRACT_POLICY_ENFORCE").unwrap_or_default())
        .unwrap_or(false)
}

fn x402_enabled(override_value: Option<bool>) -> bool {
    if let Some(value) = override_value {
        return value;
    }
    parse_bool_value(&std::env::var("NC_X402").unwrap_or_default()).unwrap_or(false)
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

#[derive(Debug)]
struct Preview {
    fee_estimate: String,
    effects: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct NetworkConfig {
    horizon_url: String,
    friendbot_url: Option<String>,
    soroban_network: String,
    soroban_source: Option<String>,
    soroban_cli: String,
    soroban_simulate_args: Vec<String>,
    txrep_preview: bool,
}

#[derive(Debug, Clone, Default)]
struct RuntimeSettings {
    allowlist_assets: Option<String>,
    allowlist_contracts: Option<String>,
    allowlist_enforce: Option<bool>,
    contract_policy: Option<String>,
    contract_policy_dir: Option<String>,
    contract_policy_enforce: Option<bool>,
    x402: Option<bool>,
}

impl RuntimeSettings {
    fn allowlist(&self) -> Allowlist {
        let assets = self
            .allowlist_assets
            .clone()
            .unwrap_or_else(|| env::var("NC_ASSET_ALLOWLIST").unwrap_or_default());
        let contracts = self
            .allowlist_contracts
            .clone()
            .unwrap_or_else(|| env::var("NC_SOROBAN_ALLOWLIST").unwrap_or_default());
        Allowlist::from_raw(&assets, &contracts)
    }
}

fn runtime_settings_from_env() -> RuntimeSettings {
    RuntimeSettings {
        allowlist_assets: env::var("NC_ASSET_ALLOWLIST")
            .ok()
            .filter(|v| !v.trim().is_empty()),
        allowlist_contracts: env::var("NC_SOROBAN_ALLOWLIST")
            .ok()
            .filter(|v| !v.trim().is_empty()),
        allowlist_enforce: parse_bool_value(&env::var("NC_ALLOWLIST_ENFORCE").unwrap_or_default()),
        contract_policy: env::var("NC_CONTRACT_POLICY")
            .ok()
            .filter(|v| !v.trim().is_empty()),
        contract_policy_dir: env::var("NC_CONTRACT_POLICY_DIR")
            .ok()
            .filter(|v| !v.trim().is_empty()),
        contract_policy_enforce: parse_bool_value(
            &env::var("NC_CONTRACT_POLICY_ENFORCE").unwrap_or_default(),
        ),
        x402: parse_bool_value(&env::var("NC_X402").unwrap_or_default()),
    }
}

fn parse_simulate_args(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut parts: Vec<String> = trimmed
        .split_whitespace()
        .map(|part| part.to_string())
        .collect();
    if parts.len() == 1 && parts[0] == "--send" {
        parts.push("no".to_string());
    }
    parts
}

fn default_horizon_url(network: &str) -> String {
    match network {
        "public" | "pubnet" | "mainnet" => "https://horizon.stellar.org".to_string(),
        _ => "https://horizon-testnet.stellar.org".to_string(),
    }
}

fn default_friendbot_url(network: &str) -> Option<String> {
    match network {
        "testnet" => Some("https://friendbot.stellar.org".to_string()),
        _ => None,
    }
}

fn load_network_config() -> NetworkConfig {
    let network = env::var("NC_STELLAR_NETWORK")
        .or_else(|_| env::var("NC_SOROBAN_NETWORK"))
        .unwrap_or_else(|_| "testnet".to_string());

    let horizon_url =
        env::var("NC_STELLAR_HORIZON_URL").unwrap_or_else(|_| default_horizon_url(&network));

    let friendbot_url = env::var("NC_FRIENDBOT_URL")
        .ok()
        .or_else(|| default_friendbot_url(&network));

    let soroban_source = env::var("NC_SOROBAN_SOURCE")
        .or_else(|_| env::var("NC_STELLAR_SOURCE"))
        .ok();

    let soroban_cli = env::var("NC_STELLAR_CLI").unwrap_or_else(|_| "stellar".to_string());
    let soroban_simulate_raw =
        env::var("NC_SOROBAN_SIMULATE_FLAG").unwrap_or_else(|_| "--send no".to_string());
    let soroban_simulate_args = parse_simulate_args(&soroban_simulate_raw);
    let txrep_preview = matches!(
        env::var("NC_TXREP_PREVIEW")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    );

    NetworkConfig {
        horizon_url,
        friendbot_url,
        soroban_network: network,
        soroban_source,
        soroban_cli,
        soroban_simulate_args,
        txrep_preview,
    }
}

fn load_contract_policies(runtime: Option<&RuntimeSettings>) -> Vec<ContractPolicy> {
    let mut policies = Vec::new();

    let direct_policy_path = runtime
        .and_then(|r| r.contract_policy.as_deref())
        .map(str::to_string)
        .or_else(|| env::var("NC_CONTRACT_POLICY").ok())
        .filter(|v| !v.trim().is_empty());
    if let Some(path) = direct_policy_path {
        if let Ok(data) = fs::read_to_string(&path) {
            match serde_json::from_str::<ContractPolicy>(&data) {
                Ok(policy) => policies.push(policy),
                Err(err) => eprintln!("Policy parse failed for {path}: {err}"),
            }
        } else {
            eprintln!("Policy file not found: {path}");
        }
    }

    let policy_dir = runtime
        .and_then(|r| r.contract_policy_dir.as_deref())
        .map(str::to_string)
        .or_else(|| env::var("NC_CONTRACT_POLICY_DIR").ok())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "contracts".to_string());
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

fn is_strkey_with_prefixes(value: &str, prefixes: &[char]) -> bool {
    if value.len() != 56 {
        return false;
    }
    let first = value.chars().next().unwrap_or('\0');
    if !prefixes.contains(&first) {
        return false;
    }
    value.chars().all(is_base32_char)
}

fn is_strkey(value: &str) -> bool {
    is_strkey_with_prefixes(value, &['G', 'C'])
}

fn extract_strkey_with_prefixes(text: &str, prefixes: &[char]) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut candidate = String::new();
    for ch in trimmed.chars() {
        let upper = ch.to_ascii_uppercase();
        if is_base32_char(upper) {
            candidate.push(upper);
        } else {
            if is_strkey_with_prefixes(&candidate, prefixes) {
                return Some(candidate);
            }
            candidate.clear();
        }
    }
    if is_strkey_with_prefixes(&candidate, prefixes) {
        return Some(candidate);
    }
    None
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
            neurochain::actions::Action::SorobanContractInvoke {
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
            *action = neurochain::actions::Action::Unknown {
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
        if let neurochain::actions::Action::SorobanContractInvoke {
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
                let args_obj = args_obj.unwrap();

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

fn estimate_op_count(plan: &ActionPlan) -> usize {
    plan.actions
        .iter()
        .filter(|action| {
            matches!(
                action.kind(),
                "stellar.account.create"
                    | "stellar.change_trust"
                    | "stellar.payment"
                    | "soroban.contract.deploy"
                    | "soroban.contract.invoke"
            )
        })
        .count()
}

fn fetch_base_fee(client: &Client, horizon_url: &str) -> Option<u64> {
    let url = format!("{}/fee_stats", horizon_url.trim_end_matches('/'));
    let resp = client.get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: Value = resp.json().ok()?;
    json.get("last_ledger_base_fee")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u64>().ok())
}

fn fetch_tx_status(client: &Client, horizon_url: &str, hash: &str) -> Result<String> {
    let url = format!(
        "{}/transactions/{}",
        horizon_url.trim_end_matches('/'),
        hash
    );
    let resp = client
        .get(url)
        .send()
        .context("horizon tx request failed")?;
    if resp.status().as_u16() == 404 {
        return Err(anyhow!("transaction not found"));
    }
    if !resp.status().is_success() {
        return Err(anyhow!("horizon tx error: {}", resp.status()));
    }
    let json: Value = resp.json().context("failed to parse horizon tx response")?;
    let successful = json
        .get("successful")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let ledger = json
        .get("ledger")
        .and_then(|v| v.as_i64())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(format!(
        "tx {hash}: successful={successful} ledger={ledger}"
    ))
}

fn parse_amount_to_stroops(raw: &str) -> Result<String> {
    let cleaned = raw.trim().replace('_', "");
    if cleaned.is_empty() {
        return Err(anyhow!("amount is empty"));
    }
    if !cleaned.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(anyhow!("amount must be numeric"));
    }
    let mut parts = cleaned.splitn(2, '.');
    let whole = parts.next().unwrap_or("0");
    let frac = parts.next().unwrap_or("");
    if frac.len() > 7 {
        return Err(anyhow!("amount has more than 7 decimal places"));
    }
    let mut frac_padded = frac.to_string();
    while frac_padded.len() < 7 {
        frac_padded.push('0');
    }
    let whole_val: u128 = whole.parse().unwrap_or(0);
    let frac_val: u128 = if frac_padded.is_empty() {
        0
    } else {
        frac_padded.parse().unwrap_or(0)
    };
    let stroops = whole_val
        .saturating_mul(10_000_000u128)
        .saturating_add(frac_val);
    Ok(stroops.to_string())
}

fn normalize_cli_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stdout.is_empty() && !stderr.is_empty() {
        return stderr;
    }
    stdout
}

fn extract_tx_hash(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(json) = serde_json::from_str::<Value>(trimmed) {
        for key in ["hash", "tx_hash", "transaction_hash", "envelope_hash"] {
            if let Some(hash) = json.get(key).and_then(|v| v.as_str()) {
                if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(hash.to_string());
                }
            }
        }
    }

    let mut candidate = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_hexdigit() {
            candidate.push(ch);
        } else {
            if candidate.len() == 64 {
                return Some(candidate);
            }
            candidate.clear();
        }
    }
    if candidate.len() == 64 {
        return Some(candidate);
    }
    None
}

fn extract_contract_id(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(json) = serde_json::from_str::<Value>(trimmed) {
        for key in ["contract_id", "id"] {
            if let Some(contract_id) = json.get(key).and_then(|v| v.as_str()) {
                if is_strkey(contract_id) && contract_id.starts_with('C') {
                    return Some(contract_id.to_string());
                }
            }
        }
    }

    let mut candidate = String::new();
    for ch in trimmed.chars() {
        if is_base32_char(ch) || ch == 'C' {
            candidate.push(ch);
        } else {
            if candidate.len() == 56 && candidate.starts_with('C') && is_strkey(&candidate) {
                return Some(candidate);
            }
            candidate.clear();
        }
    }

    if candidate.len() == 56 && candidate.starts_with('C') && is_strkey(&candidate) {
        return Some(candidate);
    }

    None
}

fn try_hash_via_cli(cfg: &NetworkConfig, xdr: &str) -> Option<String> {
    if xdr.trim().is_empty() {
        return None;
    }
    let output = Command::new(&cfg.soroban_cli)
        .args([
            "tx",
            "hash",
            "--xdr",
            xdr,
            "--network",
            &cfg.soroban_network,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    extract_tx_hash(&stdout).or_else(|| {
        if stdout.len() == 64 && stdout.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(stdout)
        } else {
            None
        }
    })
}

fn normalize_return(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    let single_line = trimmed.replace('\n', "\\n");
    Some(single_line)
}

fn format_submit_ok(label: &str, hash: Option<String>, output: &str, note: Option<&str>) -> String {
    let hash_text = hash.unwrap_or_else(|| "-".to_string());
    let mut return_text = normalize_return(output).unwrap_or_else(|| "-".to_string());
    if let Some(note) = note {
        return_text = format!("{return_text} ({note})");
    }
    format!("{label} | status=ok | tx_hash={hash_text} | return={return_text}")
}

fn format_submit_error(label: &str, stage: &str, err: &str) -> String {
    let err_text = err.trim().replace('\n', "\\n");
    format!("{label} | status=error | stage={stage} | error={err_text}")
}

fn run_stellar_cli_capture(cfg: &NetworkConfig, args: &[&str]) -> Result<String> {
    let output = Command::new(&cfg.soroban_cli)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            normalize_cli_output(&output)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn generate_wallet_alias(cfg: &NetworkConfig, alias: &str) -> Result<String> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Err(anyhow!("wallet alias is empty"));
    }

    run_stellar_cli_capture(cfg, &["keys", "generate", alias])
        .with_context(|| format!("key generation failed for alias `{alias}`"))?;

    let addr_output = run_stellar_cli_capture(cfg, &["keys", "address", alias])
        .with_context(|| format!("failed to read address for alias `{alias}`"))?;
    extract_strkey_with_prefixes(&addr_output, &['G'])
        .ok_or_else(|| anyhow!("could not parse public key from `stellar keys address` output"))
}

fn bootstrap_wallet_alias(cfg: &NetworkConfig, alias: &str) -> Result<(String, String)> {
    let public_key = generate_wallet_alias(cfg, alias)?;
    let friendbot_url = cfg
        .friendbot_url
        .as_deref()
        .ok_or_else(|| anyhow!("friendbot unavailable (set friendbot URL or use setup testnet)"))?;
    let client = Client::new();
    let fund_msg = friendbot_fund(&client, friendbot_url, &public_key)
        .with_context(|| format!("friendbot fund failed for `{public_key}`"))?;
    Ok((public_key, fund_msg))
}

fn resolve_horizon_account_from_source(cfg: &NetworkConfig, source: &str) -> Option<String> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    if source.starts_with('G') && is_strkey(source) {
        return Some(source.to_string());
    }

    let addr_output = run_stellar_cli_capture(cfg, &["keys", "address", source]).ok()?;
    extract_strkey_with_prefixes(&addr_output, &['G'])
}

fn stellar_tx_new(cfg: &NetworkConfig, args: &[String]) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_deref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE/NC_STELLAR_SOURCE not set"))?;
    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.arg("tx")
        .arg("new")
        .args(args)
        .arg("--source-account")
        .arg(source)
        .arg("--network")
        .arg(&cfg.soroban_network);
    let output = cmd
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn stellar_tx_build_only(cfg: &NetworkConfig, args: &[String]) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_deref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE/NC_STELLAR_SOURCE not set"))?;
    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.arg("tx")
        .arg("new")
        .args(args)
        .arg("--source-account")
        .arg(source)
        .arg("--network")
        .arg(&cfg.soroban_network)
        .arg("--build-only");
    let output = cmd
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn soroban_cli_build(
    cfg: &NetworkConfig,
    contract_id: &str,
    function: &str,
    args: &Value,
) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_ref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE is not set"))?;

    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.args([
        "contract",
        "invoke",
        "--id",
        contract_id,
        "--source",
        source,
        "--network",
        &cfg.soroban_network,
        "--build-only",
    ]);
    cmd.arg("--");
    cmd.arg(function);
    for (key, value) in args_to_cli(args) {
        cmd.arg(format!("--{key}")).arg(value);
    }
    let output = cmd.output().context("failed to run stellar CLI")?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn xdr_to_txrep(cfg: &NetworkConfig, xdr: &str) -> Result<String> {
    if xdr.trim().is_empty() {
        return Err(anyhow!("empty xdr"));
    }
    let output = Command::new(&cfg.soroban_cli)
        .args(["tx", "to-rep", "--xdr", xdr])
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    // Fallback for CLI versions without `tx to-rep`.
    let fallback = Command::new(&cfg.soroban_cli)
        .args(["tx", "decode", "--output", "json-formatted", xdr])
        .output()
        .with_context(|| format!("failed to run {}", cfg.soroban_cli))?;
    if !fallback.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&fallback.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&fallback.stdout).trim().to_string())
}

fn fetch_account(client: &Client, horizon_url: &str, account: &str) -> Result<Value> {
    let url = format!("{}/accounts/{}", horizon_url.trim_end_matches('/'), account);
    let resp = client.get(url).send().context("horizon request failed")?;
    if resp.status().as_u16() == 404 {
        return Err(anyhow!("account not found"));
    }
    if !resp.status().is_success() {
        return Err(anyhow!("horizon error: {}", resp.status()));
    }
    Ok(resp.json::<Value>()?)
}

fn fetch_latest_tx_hash(client: &Client, horizon_url: &str, account: &str) -> Result<String> {
    let url = format!(
        "{}/accounts/{}/transactions?limit=1&order=desc",
        horizon_url.trim_end_matches('/'),
        account
    );
    let resp = client.get(url).send().context("horizon request failed")?;
    if !resp.status().is_success() {
        return Err(anyhow!("horizon error: {}", resp.status()));
    }
    let json = resp.json::<Value>()?;
    let record = json
        .get("_embedded")
        .and_then(|v| v.get("records"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .ok_or_else(|| anyhow!("no transactions found"))?;
    record
        .get("hash")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .ok_or_else(|| anyhow!("missing tx hash"))
}

fn fetch_balances(client: &Client, horizon_url: &str, account: &str) -> Result<Vec<String>> {
    let json = fetch_account(client, horizon_url, account)?;
    let balances = json
        .get("balances")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("missing balances"))?;

    let mut out = Vec::new();
    for entry in balances {
        let asset_type = entry
            .get("asset_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let balance = entry.get("balance").and_then(|v| v.as_str()).unwrap_or("");
        let label = if asset_type == "native" {
            "XLM".to_string()
        } else {
            let code = entry
                .get("asset_code")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let issuer = entry
                .get("asset_issuer")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("{code}:{issuer}")
        };
        out.push(format!("{label} = {balance}"));
    }
    Ok(out)
}

fn friendbot_fund(client: &Client, friendbot_url: &str, account: &str) -> Result<String> {
    let url = format!("{}?addr={}", friendbot_url.trim_end_matches('/'), account);
    let resp = client.get(url).send().context("friendbot request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(anyhow!("friendbot error: {} {}", status, text));
    }
    Ok("friendbot funded account".to_string())
}

fn args_to_cli(args: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(map) = args.as_object() {
        let mut keys: Vec<_> = map.keys().collect();
        keys.sort();
        for key in keys {
            let value = map.get(key).unwrap();
            let val = if let Some(s) = value.as_str() {
                s.to_string()
            } else {
                value.to_string()
            };
            out.push((key.to_string(), val));
        }
    }
    out
}

fn soroban_cli_invoke(
    cfg: &NetworkConfig,
    contract_id: &str,
    function: &str,
    args: &Value,
    simulate: bool,
) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_ref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE is not set"))?;

    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.args([
        "contract",
        "invoke",
        "--id",
        contract_id,
        "--source",
        source,
        "--network",
        &cfg.soroban_network,
    ]);
    if simulate {
        cmd.args(&cfg.soroban_simulate_args);
    }
    cmd.arg("--");
    cmd.arg(function);
    for (key, value) in args_to_cli(args) {
        cmd.arg(format!("--{key}")).arg(value);
    }
    let output = cmd.output().context("failed to run stellar CLI")?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn soroban_cli_deploy(
    cfg: &NetworkConfig,
    alias: &str,
    wasm: &str,
    simulate: bool,
) -> Result<String> {
    let source = cfg
        .soroban_source
        .as_ref()
        .ok_or_else(|| anyhow!("NC_SOROBAN_SOURCE is not set"))?;

    let mut cmd = Command::new(&cfg.soroban_cli);
    cmd.args([
        "contract",
        "deploy",
        "--source-account",
        source,
        "--network",
        &cfg.soroban_network,
        "--alias",
        alias,
        "--wasm",
        wasm,
    ]);
    if simulate {
        cmd.args(&cfg.soroban_simulate_args);
    }
    let output = cmd.output().context("failed to run stellar CLI")?;
    if !output.status.success() {
        return Err(anyhow!(
            "stellar CLI error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(normalize_cli_output(&output))
}

fn simulate_plan(plan: &ActionPlan, cfg: &NetworkConfig) -> Preview {
    let client = Client::new();
    let base_fee = fetch_base_fee(&client, &cfg.horizon_url).unwrap_or(100);
    let op_count = estimate_op_count(plan);
    let total_fee = base_fee.saturating_mul(op_count as u64);

    let mut effects = Vec::new();
    let mut warnings = Vec::new();

    for action in &plan.actions {
        match action {
            neurochain::actions::Action::StellarAccountBalance { account, asset } => {
                match fetch_balances(&client, &cfg.horizon_url, account) {
                    Ok(balances) => {
                        if let Some(asset) = asset {
                            let line = balances
                                .iter()
                                .find(|b| b.starts_with(asset))
                                .cloned()
                                .unwrap_or_else(|| format!("{asset} = (not found)"));
                            effects.push(format!("balance {account}: {line}"));
                        } else {
                            for line in balances {
                                effects.push(format!("balance {account}: {line}"));
                            }
                        }
                    }
                    Err(err) => {
                        warnings.push(format!("simulate_error: balance {account} failed: {err}"))
                    }
                }
            }
            neurochain::actions::Action::StellarAccountFundTestnet { account } => {
                let exists = fetch_account(&client, &cfg.horizon_url, account).is_ok();
                let msg = if exists {
                    format!("friendbot will top up existing account {account}")
                } else {
                    format!("friendbot will create and fund account {account}")
                };
                effects.push(msg);
            }
            neurochain::actions::Action::StellarAccountCreate {
                destination,
                starting_balance,
            } => {
                effects.push(format!(
                    "create account {destination} with starting_balance {starting_balance} XLM"
                ));
                if cfg.txrep_preview {
                    match parse_amount_to_stroops(starting_balance).and_then(|amount| {
                        stellar_tx_build_only(
                            cfg,
                            &[
                                "create-account".to_string(),
                                "--destination".to_string(),
                                destination.clone(),
                                "--starting-balance".to_string(),
                                amount,
                            ],
                        )
                    }) {
                        Ok(xdr) => match xdr_to_txrep(cfg, &xdr) {
                            Ok(txrep) => effects
                                .push(format!("txrep create-account {destination}:\n{txrep}")),
                            Err(err) => warnings.push(format!(
                                "preview_error: txrep create-account {destination} failed: {err}"
                            )),
                        },
                        Err(err) => warnings.push(format!(
                            "preview_error: txrep create-account {destination} failed: {err}"
                        )),
                    }
                }
            }
            neurochain::actions::Action::StellarChangeTrust {
                asset_code,
                asset_issuer,
                limit,
            } => {
                let mut line = format!("change trust {}:{}", asset_code, asset_issuer);
                if let Some(limit) = limit {
                    line.push_str(&format!(" limit {limit}"));
                }
                effects.push(line);
                if cfg.txrep_preview {
                    let line = format!("{asset_code}:{asset_issuer}");
                    let mut args = vec![
                        "change-trust".to_string(),
                        "--line".to_string(),
                        line.clone(),
                    ];
                    if let Some(limit) = limit {
                        match parse_amount_to_stroops(limit) {
                            Ok(limit_stroops) => {
                                args.push("--limit".to_string());
                                args.push(limit_stroops);
                            }
                            Err(err) => {
                                warnings.push(format!(
                                    "preview_error: txrep change-trust {line} failed: {err}"
                                ));
                                continue;
                            }
                        }
                    }
                    match stellar_tx_build_only(cfg, &args) {
                        Ok(xdr) => match xdr_to_txrep(cfg, &xdr) {
                            Ok(txrep) => {
                                effects.push(format!("txrep change-trust {line}:\n{txrep}"))
                            }
                            Err(err) => warnings.push(format!(
                                "preview_error: txrep change-trust {line} failed: {err}"
                            )),
                        },
                        Err(err) => warnings.push(format!(
                            "preview_error: txrep change-trust {line} failed: {err}"
                        )),
                    }
                }
            }
            neurochain::actions::Action::StellarPayment {
                to,
                amount,
                asset_code,
                asset_issuer,
            } => {
                let asset = if asset_code.eq_ignore_ascii_case("XLM") && asset_issuer.is_none() {
                    "native".to_string()
                } else if let Some(issuer) = asset_issuer {
                    format!("{}:{}", asset_code, issuer)
                } else {
                    asset_code.clone()
                };
                effects.push(format!("payment {amount} {asset} -> {to}"));
                if cfg.txrep_preview {
                    match parse_amount_to_stroops(amount).and_then(|amount_stroops| {
                        stellar_tx_build_only(
                            cfg,
                            &[
                                "payment".to_string(),
                                "--destination".to_string(),
                                to.clone(),
                                "--asset".to_string(),
                                asset.clone(),
                                "--amount".to_string(),
                                amount_stroops,
                            ],
                        )
                    }) {
                        Ok(xdr) => match xdr_to_txrep(cfg, &xdr) {
                            Ok(txrep) => effects
                                .push(format!("txrep payment {amount} {asset} -> {to}:\n{txrep}")),
                            Err(err) => warnings.push(format!(
                                "preview_error: txrep payment {amount} {asset} -> {to} failed: {err}"
                            )),
                        },
                        Err(err) => warnings.push(format!(
                            "preview_error: txrep payment {amount} {asset} -> {to} failed: {err}"
                        )),
                    }
                }
            }
            neurochain::actions::Action::StellarTxStatus { hash } => {
                match fetch_tx_status(&client, &cfg.horizon_url, hash) {
                    Ok(status) => effects.push(status),
                    Err(err) => warnings.push(format!("simulate_error: tx status failed: {err}")),
                }
            }
            neurochain::actions::Action::SorobanContractDeploy { alias, wasm } => {
                effects.push(format!("soroban deploy alias={alias} wasm={wasm}"));
                if !std::path::Path::new(wasm).exists() {
                    warnings.push(format!(
                        "simulate_error: soroban deploy alias={alias} missing wasm file: {wasm}"
                    ));
                    continue;
                }
                match soroban_cli_deploy(cfg, alias, wasm, true) {
                    Ok(output) => {
                        if output.trim().is_empty() {
                            effects.push(format!("soroban simulate deploy alias={alias} -> ok"));
                        } else {
                            effects
                                .push(format!("soroban simulate deploy alias={alias} -> {output}"));
                        }
                    }
                    Err(err) => warnings.push(format!(
                        "simulate_error: soroban deploy alias={alias} failed: {err}"
                    )),
                }
            }
            neurochain::actions::Action::SorobanContractInvoke {
                contract_id,
                function,
                args,
            } => match soroban_cli_invoke(cfg, contract_id, function, args, true) {
                Ok(output) => {
                    if output.trim().is_empty() {
                        effects.push(format!("soroban simulate {contract_id}:{function} -> ok"));
                    } else {
                        effects.push(format!(
                            "soroban simulate {contract_id}:{function} -> {output}"
                        ));
                    }
                    if cfg.txrep_preview {
                        match soroban_cli_build(cfg, contract_id, function, args) {
                            Ok(xdr) => match xdr_to_txrep(cfg, &xdr) {
                                Ok(txrep) => effects.push(format!(
                                    "txrep soroban {contract_id}:{function}:\n{txrep}"
                                )),
                                Err(err) => warnings.push(format!(
                                    "preview_error: txrep soroban {contract_id}:{function} failed: {err}"
                                )),
                            },
                            Err(err) => warnings.push(format!(
                                "preview_error: txrep soroban {contract_id}:{function} failed: {err}"
                            )),
                        }
                    }
                }
                Err(err) => warnings.push(format!(
                    "simulate_error: soroban {contract_id}:{function} failed: {err}"
                )),
            },
            other => warnings.push(format!(
                "simulate_skip: not implemented for {}",
                other.kind()
            )),
        }
    }

    Preview {
        fee_estimate: format!("{base_fee} stroops x {op_count} ops = {total_fee} stroops"),
        effects,
        warnings,
    }
}

fn print_preview(preview: &Preview) {
    eprintln!("=== Preview ===");
    eprintln!("Estimated fee: {}", preview.fee_estimate);
    if preview.effects.is_empty() {
        eprintln!("Effects: (none)");
    } else {
        eprintln!("Effects:");
        for effect in &preview.effects {
            eprintln!("  - {effect}");
        }
    }
    if !preview.warnings.is_empty() {
        eprintln!("Warnings:");
        for warning in &preview.warnings {
            eprintln!("  - {warning}");
        }
    }
}

fn confirm_submit(auto_yes: bool) -> bool {
    if auto_yes {
        return true;
    }
    eprint!("Confirm submit? [y/N]: ");
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn submit_plan(plan: &ActionPlan, cfg: &NetworkConfig) -> Vec<String> {
    let client = Client::new();
    let mut outputs = Vec::new();

    for action in &plan.actions {
        match action {
            neurochain::actions::Action::StellarAccountFundTestnet { account } => {
                if let Some(friendbot_url) = cfg.friendbot_url.as_deref() {
                    match friendbot_fund(&client, friendbot_url, account) {
                        Ok(msg) => outputs.push(format!("{account}: {msg}")),
                        Err(err) => outputs.push(format!("{account}: friendbot failed: {err}")),
                    }
                } else {
                    outputs.push(format!("{account}: friendbot unavailable (not testnet)"));
                }
            }
            neurochain::actions::Action::StellarAccountBalance { account, asset } => {
                match fetch_balances(&client, &cfg.horizon_url, account) {
                    Ok(balances) => {
                        if let Some(asset) = asset {
                            let line = balances
                                .iter()
                                .find(|b| b.starts_with(asset))
                                .cloned()
                                .unwrap_or_else(|| format!("{asset} = (not found)"));
                            outputs.push(format!("balance {account}: {line}"));
                        } else {
                            for line in balances {
                                outputs.push(format!("balance {account}: {line}"));
                            }
                        }
                    }
                    Err(err) => outputs.push(format!("balance submit failed for {account}: {err}")),
                }
            }
            neurochain::actions::Action::SorobanContractDeploy { alias, wasm } => {
                if !std::path::Path::new(wasm).exists() {
                    outputs.push(format_submit_error(
                        &format!("soroban deploy alias={alias}"),
                        "submit",
                        &format!("wasm file not found: {wasm}"),
                    ));
                    continue;
                }

                match soroban_cli_deploy(cfg, alias, wasm, false) {
                    Ok(output) => {
                        let mut tx_hash = extract_tx_hash(&output);
                        if tx_hash.is_none() {
                            if let Some(source) = cfg.soroban_source.as_deref() {
                                if let Some(account) =
                                    resolve_horizon_account_from_source(cfg, source)
                                {
                                    if let Ok(latest) =
                                        fetch_latest_tx_hash(&client, &cfg.horizon_url, &account)
                                    {
                                        tx_hash = Some(latest);
                                    }
                                }
                            }
                        }
                        let contract_id =
                            extract_contract_id(&output).unwrap_or_else(|| "-".to_string());
                        let return_text =
                            normalize_return(&output).unwrap_or_else(|| "-".to_string());
                        outputs.push(format!(
                            "soroban deploy alias={alias} | status=ok | contract_id={contract_id} | tx_hash={} | return={return_text}",
                            tx_hash.unwrap_or_else(|| "-".to_string())
                        ));
                    }
                    Err(err) => outputs.push(format_submit_error(
                        &format!("soroban deploy alias={alias}"),
                        "submit",
                        &err.to_string(),
                    )),
                }
            }
            neurochain::actions::Action::SorobanContractInvoke {
                contract_id,
                function,
                args,
            } => match soroban_cli_invoke(cfg, contract_id, function, args, false) {
                Ok(output) => {
                    let mut hash = extract_tx_hash(&output);
                    let mut note = None;
                    if hash.is_none() {
                        if let Some(source) = cfg.soroban_source.as_deref() {
                            if let Some(account) = resolve_horizon_account_from_source(cfg, source)
                            {
                                if let Ok(latest) =
                                    fetch_latest_tx_hash(&client, &cfg.horizon_url, &account)
                                {
                                    hash = Some(latest);
                                    note = Some("latest");
                                }
                            }
                        }
                    }
                    outputs.push(format_submit_ok(
                        &format!("soroban submit {contract_id}:{function}"),
                        hash,
                        &output,
                        note,
                    ));
                }
                Err(err) => outputs.push(format_submit_error(
                    &format!("soroban submit {contract_id}:{function}"),
                    "submit",
                    &err.to_string(),
                )),
            },
            neurochain::actions::Action::StellarAccountCreate {
                destination,
                starting_balance,
            } => match parse_amount_to_stroops(starting_balance).and_then(|amount| {
                stellar_tx_new(
                    cfg,
                    &[
                        "create-account".to_string(),
                        "--destination".to_string(),
                        destination.clone(),
                        "--starting-balance".to_string(),
                        amount,
                    ],
                )
            }) {
                Ok(output) => {
                    let hash = extract_tx_hash(&output).or_else(|| try_hash_via_cli(cfg, &output));
                    outputs.push(format_submit_ok(
                        &format!("create-account {destination}"),
                        hash,
                        &output,
                        None,
                    ));
                }
                Err(err) => outputs.push(format_submit_error(
                    &format!("create-account {destination}"),
                    "submit",
                    &err.to_string(),
                )),
            },
            neurochain::actions::Action::StellarChangeTrust {
                asset_code,
                asset_issuer,
                limit,
            } => {
                let line = format!("{asset_code}:{asset_issuer}");
                let mut args = vec![
                    "change-trust".to_string(),
                    "--line".to_string(),
                    line.clone(),
                ];
                if let Some(limit) = limit {
                    match parse_amount_to_stroops(limit) {
                        Ok(limit_stroops) => {
                            args.push("--limit".to_string());
                            args.push(limit_stroops);
                        }
                        Err(err) => {
                            outputs.push(format_submit_error(
                                &format!("change-trust {line}"),
                                "submit",
                                &err.to_string(),
                            ));
                            continue;
                        }
                    }
                }
                match stellar_tx_new(cfg, &args) {
                    Ok(output) => {
                        let hash =
                            extract_tx_hash(&output).or_else(|| try_hash_via_cli(cfg, &output));
                        outputs.push(format_submit_ok(
                            &format!("change-trust {line}"),
                            hash,
                            &output,
                            None,
                        ));
                    }
                    Err(err) => outputs.push(format_submit_error(
                        &format!("change-trust {line}"),
                        "submit",
                        &err.to_string(),
                    )),
                }
            }
            neurochain::actions::Action::StellarPayment {
                to,
                amount,
                asset_code,
                asset_issuer,
            } => {
                let asset = if asset_code.eq_ignore_ascii_case("XLM") && asset_issuer.is_none() {
                    "native".to_string()
                } else if let Some(issuer) = asset_issuer {
                    format!("{asset_code}:{issuer}")
                } else {
                    outputs.push(format_submit_error(
                        &format!("payment {amount} {asset_code} -> {to}"),
                        "submit",
                        &format!("missing asset_issuer for {asset_code}"),
                    ));
                    continue;
                };
                match parse_amount_to_stroops(amount).and_then(|amount_stroops| {
                    stellar_tx_new(
                        cfg,
                        &[
                            "payment".to_string(),
                            "--destination".to_string(),
                            to.clone(),
                            "--asset".to_string(),
                            asset.clone(),
                            "--amount".to_string(),
                            amount_stroops,
                        ],
                    )
                }) {
                    Ok(output) => {
                        let hash =
                            extract_tx_hash(&output).or_else(|| try_hash_via_cli(cfg, &output));
                        outputs.push(format_submit_ok(
                            &format!("payment {amount} {asset} -> {to}"),
                            hash,
                            &output,
                            None,
                        ));
                    }
                    Err(err) => outputs.push(format_submit_error(
                        &format!("payment {amount} {asset} -> {to}"),
                        "submit",
                        &err.to_string(),
                    )),
                }
            }
            neurochain::actions::Action::StellarTxStatus { hash } => {
                match fetch_tx_status(&client, &cfg.horizon_url, hash) {
                    Ok(status) => outputs.push(status),
                    Err(err) => outputs.push(format!("tx status failed for {hash}: {err}")),
                }
            }
            other => outputs.push(format!("submit not implemented for {}", other.kind())),
        }
    }

    outputs
}

fn print_intent_block_reasons(plan: &ActionPlan) {
    for warning in &plan.warnings {
        if warning.starts_with("intent_error:") || warning.starts_with("intent_warning:") {
            eprintln!("- {warning}");
        }
    }
    for action in &plan.actions {
        if let Action::Unknown { reason } = action {
            eprintln!("- intent_block: {reason}");
        }
    }
}

fn strip_wrapping_quotes(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

fn parse_ai_model_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let (left, right) = trimmed.split_once(':')?;
    if !left.trim().eq_ignore_ascii_case("AI") {
        return None;
    }
    let model_path = strip_wrapping_quotes(right);
    if model_path.is_empty() {
        None
    } else {
        Some(model_path)
    }
}

fn extract_prompt_from_macro_from_ai(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("macro from ai:") {
        return None;
    }
    let idx = trimmed.find(':')?;
    let prompt = strip_wrapping_quotes(&trimmed[idx + 1..]);
    if prompt.is_empty() {
        None
    } else {
        Some(prompt)
    }
}

fn is_macro_from_ai_line(line: &str) -> bool {
    extract_prompt_from_macro_from_ai(line).is_some()
}

fn parse_named_value(line: &str, names: &[&str]) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    for name in names {
        let name_l = name.to_ascii_lowercase();

        let prefix = format!("{name_l}:");
        if lower.starts_with(&prefix) {
            let value = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !value.is_empty() {
                return Some(value);
            }
        }

        let prefix = format!("{name_l}=");
        if lower.starts_with(&prefix) {
            let value = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !value.is_empty() {
                return Some(value);
            }
        }

        let prefix = format!("{name_l} ");
        if lower.starts_with(&prefix) {
            let value = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !value.is_empty() {
                return Some(value);
            }
        }

        let prefix = format!("set {name_l} =");
        if lower.starts_with(&prefix) {
            let value = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn parse_network_line(line: &str) -> Option<String> {
    parse_named_value(line, &["network"])
}

fn parse_source_line(line: &str) -> Option<String> {
    parse_named_value(line, &["wallet", "source", "lompakko"])
}

fn parse_wallet_generate_line(line: &str) -> Option<String> {
    if let Some(alias) = parse_named_value(
        line,
        &[
            "wallet_generate",
            "generate_wallet",
            "wallet_create",
            "keygen",
        ],
    ) {
        return Some(alias);
    }

    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["wallet generate ", "keys generate ", "generate wallet "] {
        if lower.starts_with(prefix) {
            let alias = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !alias.is_empty() {
                return Some(alias);
            }
        }
    }
    None
}

fn parse_wallet_bootstrap_line(line: &str) -> Option<String> {
    if let Some(alias) = parse_named_value(line, &["wallet_bootstrap", "bootstrap_wallet"]) {
        return Some(alias);
    }

    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["wallet bootstrap ", "bootstrap wallet "] {
        if lower.starts_with(prefix) {
            let alias = strip_wrapping_quotes(trimmed[prefix.len()..].trim_start());
            if !alias.is_empty() {
                return Some(alias);
            }
        }
    }
    None
}

fn parse_horizon_line(line: &str) -> Option<String> {
    parse_named_value(line, &["horizon", "horizon_url"])
}

fn parse_friendbot_line(line: &str) -> Option<Option<String>> {
    let value = parse_named_value(line, &["friendbot", "friendbot_url"])?;
    let lowered = value.trim().to_ascii_lowercase();
    if matches!(lowered.as_str(), "off" | "none" | "null" | "disabled") {
        return Some(None);
    }
    Some(Some(value))
}

fn parse_stellar_cli_line(line: &str) -> Option<String> {
    parse_named_value(line, &["stellar_cli", "cli"])
}

fn parse_simulate_flag_line(line: &str) -> Option<String> {
    parse_named_value(line, &["simulate_flag", "soroban_simulate_flag"])
}

fn parse_txrep_line(line: &str) -> Result<Option<bool>> {
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("txrep") {
        return Ok(Some(true));
    }
    let Some(value) = parse_named_value(line, &["txrep", "txrep_preview"]) else {
        return Ok(None);
    };
    parse_bool_value(&value)
        .map(Some)
        .ok_or_else(|| anyhow!("invalid txrep value `{value}` (use txrep, txrep on, txrep off)"))
}

fn parse_intent_threshold_line(line: &str) -> Result<Option<f32>> {
    let Some(value) = parse_named_value(line, &["intent_threshold"]) else {
        return Ok(None);
    };
    let parsed = value
        .parse::<f32>()
        .with_context(|| format!("invalid intent_threshold value: {value}"))?;
    Ok(Some(parsed))
}

fn parse_asset_allowlist_line(line: &str) -> Option<String> {
    parse_named_value(line, &["asset_allowlist", "allowlist_assets"])
}

fn parse_contract_allowlist_line(line: &str) -> Option<String> {
    parse_named_value(
        line,
        &[
            "soroban_allowlist",
            "contract_allowlist",
            "allowlist_contracts",
        ],
    )
}

fn parse_allowlist_enforce_line(line: &str) -> Result<Option<bool>> {
    if line.trim().eq_ignore_ascii_case("allowlist_enforce") {
        return Ok(Some(true));
    }
    let Some(value) = parse_named_value(line, &["allowlist_enforce"]) else {
        return Ok(None);
    };
    parse_bool_value(&value)
        .map(Some)
        .ok_or_else(|| anyhow!("invalid allowlist_enforce value `{value}` (use on/off/true/false)"))
}

fn parse_contract_policy_line(line: &str) -> Option<Option<String>> {
    let value = parse_named_value(line, &["contract_policy", "policy_file", "policy"])?;
    let lowered = value.trim().to_ascii_lowercase();
    if matches!(lowered.as_str(), "off" | "none" | "null" | "disabled") {
        return Some(None);
    }
    Some(Some(value))
}

fn parse_contract_policy_dir_line(line: &str) -> Option<Option<String>> {
    let value = parse_named_value(line, &["contract_policy_dir", "policy_dir"])?;
    let lowered = value.trim().to_ascii_lowercase();
    if matches!(lowered.as_str(), "off" | "none" | "null" | "disabled") {
        return Some(None);
    }
    Some(Some(value))
}

fn parse_contract_policy_enforce_line(line: &str) -> Result<Option<bool>> {
    if line.trim().eq_ignore_ascii_case("contract_policy_enforce")
        || line.trim().eq_ignore_ascii_case("policy_enforce")
    {
        return Ok(Some(true));
    }
    let Some(value) = parse_named_value(line, &["contract_policy_enforce", "policy_enforce"])
    else {
        return Ok(None);
    };
    parse_bool_value(&value).map(Some).ok_or_else(|| {
        anyhow!("invalid contract_policy_enforce value `{value}` (use on/off/true/false)")
    })
}

fn parse_debug_line(line: &str) -> Result<Option<bool>> {
    if line.trim().eq_ignore_ascii_case("debug") {
        return Ok(Some(true));
    }
    let Some(value) = parse_named_value(line, &["debug", "intent_debug"]) else {
        return Ok(None);
    };
    parse_bool_value(&value)
        .map(Some)
        .ok_or_else(|| anyhow!("invalid debug value `{value}` (use on/off/true/false)"))
}

fn parse_x402_line(line: &str) -> Result<Option<bool>> {
    if line.trim().eq_ignore_ascii_case("x402") {
        return Ok(Some(true));
    }
    let Some(value) = parse_named_value(line, &["x402"]) else {
        return Ok(None);
    };
    parse_bool_value(&value)
        .map(Some)
        .ok_or_else(|| anyhow!("invalid x402 value `{value}` (use x402, x402 on, x402 off)"))
}

fn parse_key_value_tokens(raw: &str) -> Result<HashMap<String, String>> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in raw.chars() {
        if let Some(active_quote) = quote {
            current.push(ch);
            if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            current.push(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            continue;
        }
        current.push(ch);
    }
    if quote.is_some() {
        return Err(anyhow!("unterminated quoted value in x402 command"));
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    let mut out = HashMap::new();
    for token in tokens {
        let Some((name, value_raw)) = token.split_once('=') else {
            return Err(anyhow!(
                "invalid x402 argument `{token}` (use key=\"value\")"
            ));
        };
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(anyhow!("invalid x402 argument key in `{token}`"));
        }
        let value = strip_wrapping_quotes(value_raw);
        if value.is_empty() {
            return Err(anyhow!("invalid x402 argument `{name}`: empty value"));
        }
        out.insert(name, value);
    }
    Ok(out)
}

fn parse_x402_request_line(line: &str) -> Result<Option<Action>> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    let mut rest: Option<&str> = None;
    for prefix in ["x402.request", "x402 request"] {
        if lower.starts_with(prefix) {
            rest = Some(trimmed[prefix.len()..].trim_start());
            break;
        }
    }
    let Some(rest) = rest else {
        return Ok(None);
    };
    if rest.is_empty() {
        return Err(anyhow!(
            "x402.request requires payment fields (example: x402.request to=\"G...\" amount=\"1\" asset_code=\"XLM\")"
        ));
    }
    let args = parse_key_value_tokens(rest)?;
    let to = args
        .get("to")
        .cloned()
        .ok_or_else(|| anyhow!("x402.request missing required field: to"))?;
    let amount = args
        .get("amount")
        .cloned()
        .ok_or_else(|| anyhow!("x402.request missing required field: amount"))?;
    let mut asset_code = args
        .get("asset_code")
        .cloned()
        .or_else(|| args.get("asset").cloned())
        .ok_or_else(|| anyhow!("x402.request missing required field: asset_code (or asset)"))?;
    let mut asset_issuer = args
        .get("asset_issuer")
        .cloned()
        .or_else(|| args.get("issuer").cloned());
    if !asset_code.eq_ignore_ascii_case("XLM") {
        let parsed_code_and_issuer = asset_code
            .split_once(':')
            .map(|(code, issuer)| (code.trim().to_string(), issuer.trim().to_string()));
        if let Some((code, issuer)) = parsed_code_and_issuer {
            if !code.is_empty() && !issuer.is_empty() {
                asset_code = code;
                if asset_issuer.is_none() {
                    asset_issuer = Some(issuer);
                }
            }
        }
    } else {
        asset_issuer = None;
    }
    Ok(Some(Action::StellarPayment {
        to,
        amount,
        asset_code,
        asset_issuer,
    }))
}

fn parse_x402_finalize_line(line: &str) -> Result<Option<String>> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    let mut rest: Option<&str> = None;
    for prefix in ["x402.finalize", "x402 finalize"] {
        if lower.starts_with(prefix) {
            rest = Some(trimmed[prefix.len()..].trim_start());
            break;
        }
    }
    let Some(rest) = rest else {
        return Ok(None);
    };
    if rest.is_empty() {
        return Ok(Some("last".to_string()));
    }
    if rest.eq_ignore_ascii_case("last") {
        return Ok(Some("last".to_string()));
    }
    let args = parse_key_value_tokens(rest)?;
    if let Some(challenge_id) = args
        .get("challenge_id")
        .cloned()
        .or_else(|| args.get("id").cloned())
    {
        return Ok(Some(challenge_id));
    }
    Err(anyhow!(
        "invalid x402.finalize syntax (use x402.finalize challenge_id=\"last\" or challenge_id=\"x402c0001\")"
    ))
}

#[derive(Debug, Clone)]
struct X402Challenge {
    payment: Action,
    finalized: bool,
}

#[derive(Debug, Clone, Default)]
struct X402State {
    next_id: u64,
    last_challenge_id: Option<String>,
    challenges: HashMap<String, X402Challenge>,
}

impl X402State {
    fn create_challenge(&mut self, payment: Action) -> String {
        self.next_id += 1;
        let challenge_id = format!("x402c{:04}", self.next_id);
        self.challenges.insert(
            challenge_id.clone(),
            X402Challenge {
                payment,
                finalized: false,
            },
        );
        self.last_challenge_id = Some(challenge_id.clone());
        challenge_id
    }

    fn resolve_challenge_id(&self, requested: &str) -> Option<String> {
        if requested.eq_ignore_ascii_case("last") {
            self.last_challenge_id.clone()
        } else if self.challenges.contains_key(requested) {
            Some(requested.to_string())
        } else {
            None
        }
    }
}

fn describe_stellar_payment(action: &Action) -> Option<String> {
    match action {
        Action::StellarPayment {
            to,
            amount,
            asset_code,
            asset_issuer,
        } => {
            let asset = if asset_code.eq_ignore_ascii_case("XLM") {
                "XLM".to_string()
            } else if let Some(issuer) = asset_issuer.as_deref() {
                format!("{asset_code}:{issuer}")
            } else {
                asset_code.to_string()
            };
            Some(format!("{amount} {asset} -> {to}"))
        }
        _ => None,
    }
}

fn parse_set_from_ai_assignment(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("set ") {
        return None;
    }
    let marker = " from ai:";
    let idx = lower.find(marker)?;
    let name = trimmed[4..idx].trim();
    if name.is_empty() {
        return None;
    }
    let prompt = strip_wrapping_quotes(&trimmed[idx + marker.len()..]);
    if prompt.is_empty() {
        return None;
    }
    Some((name.to_string(), prompt))
}

fn normalize_assignment_name(name: &str) -> String {
    name.split_whitespace()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn is_intent_assignment_name(name: &str) -> bool {
    matches!(
        normalize_assignment_name(name).as_str(),
        "intent" | "stellar intent"
    )
}

fn parse_set_literal_assignment(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("set ") || lower.contains(" from ai:") {
        return None;
    }
    let rest = trimmed[4..].trim_start();
    let (name, value_raw) = rest.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let value = strip_wrapping_quotes(value_raw);
    Some((name.to_string(), value))
}

fn parse_conditional_header(line: &str, keyword: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    let prefix = format!("{keyword} ");
    if !lower.starts_with(&prefix) || !trimmed.ends_with(':') {
        return None;
    }
    let content = trimmed[prefix.len()..trimmed.len() - 1].trim();
    if content.is_empty() {
        return None;
    }
    Some(content.to_string())
}

fn parse_if_header(line: &str) -> Option<String> {
    parse_conditional_header(line, "if")
}

fn parse_elif_header(line: &str) -> Option<String> {
    parse_conditional_header(line, "elif")
}

fn is_else_header(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("else:")
}

fn strip_inline_comment_outside_quotes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            out.push(ch);
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            out.push(ch);
            continue;
        }

        if ch == '#' {
            break;
        }
        // Keep URL schemes like https:// intact; treat // as comment otherwise.
        if ch == '/' && chars.peek() == Some(&'/') && !out.ends_with(':') {
            break;
        }

        out.push(ch);
    }

    out.trim_end().to_string()
}

#[derive(Debug, Clone)]
struct ScriptLine {
    indent: usize,
    line_no: usize,
    text: String,
}

fn collect_script_lines(script: &str) -> Vec<ScriptLine> {
    let mut out = Vec::new();
    for (idx, raw_line) in script.lines().enumerate() {
        let sanitized = strip_inline_comment_outside_quotes(raw_line);
        if sanitized.trim().is_empty() {
            continue;
        }
        let indent = sanitized
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .map(|ch| if ch == '\t' { 4 } else { 1 })
            .sum();
        let text = sanitized.trim().to_string();
        out.push(ScriptLine {
            indent,
            line_no: idx + 1,
            text,
        });
    }
    out
}

fn tokenize_condition(expr: &str) -> Vec<String> {
    let chars: Vec<char> = expr.chars().collect();
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut i = 0usize;
    let mut quote: Option<char> = None;

    let flush = |tokens: &mut Vec<String>, buf: &mut String| {
        let trimmed = buf.trim();
        if !trimmed.is_empty() {
            tokens.push(trimmed.to_string());
        }
        buf.clear();
    };

    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                buf.push(ch);
            }
            i += 1;
            continue;
        }

        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                i += 1;
            }
            ' ' | '\t' => {
                flush(&mut tokens, &mut buf);
                i += 1;
            }
            '=' | '!' | '>' | '<' => {
                flush(&mut tokens, &mut buf);
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(format!("{ch}="));
                    i += 2;
                } else {
                    tokens.push(ch.to_string());
                    i += 1;
                }
            }
            _ => {
                buf.push(ch);
                i += 1;
            }
        }
    }
    flush(&mut tokens, &mut buf);
    tokens
}

fn resolve_value(token: &str, vars: &HashMap<String, String>) -> String {
    let stripped = strip_wrapping_quotes(token);
    if let Some(val) = vars.get(&stripped) {
        val.clone()
    } else {
        stripped
    }
}

fn compare_values(lhs: &str, rhs: &str) -> std::cmp::Ordering {
    let l = lhs.trim();
    let r = rhs.trim();
    match (l.parse::<f64>(), r.parse::<f64>()) {
        (Ok(ln), Ok(rn)) => ln.partial_cmp(&rn).unwrap_or(std::cmp::Ordering::Equal),
        _ => l.to_ascii_lowercase().cmp(&r.to_ascii_lowercase()),
    }
}

fn eval_condition_atom(
    tokens: &[String],
    idx: &mut usize,
    vars: &HashMap<String, String>,
) -> Option<bool> {
    if *idx >= tokens.len() {
        return None;
    }

    if *idx + 2 < tokens.len() {
        let op = tokens[*idx + 1].as_str();
        if matches!(op, "==" | "!=" | ">" | "<" | ">=" | "<=") {
            let lhs = resolve_value(&tokens[*idx], vars);
            let rhs = resolve_value(&tokens[*idx + 2], vars);
            *idx += 3;
            let cmp = compare_values(&lhs, &rhs);
            return Some(match op {
                "==" => lhs.trim().eq_ignore_ascii_case(rhs.trim()),
                "!=" => !lhs.trim().eq_ignore_ascii_case(rhs.trim()),
                ">" => cmp == std::cmp::Ordering::Greater,
                "<" => cmp == std::cmp::Ordering::Less,
                ">=" => cmp == std::cmp::Ordering::Greater || cmp == std::cmp::Ordering::Equal,
                "<=" => cmp == std::cmp::Ordering::Less || cmp == std::cmp::Ordering::Equal,
                _ => false,
            });
        }
    }

    let value = resolve_value(&tokens[*idx], vars);
    *idx += 1;
    let low = value.trim().to_ascii_lowercase();
    Some(!matches!(
        low.as_str(),
        "" | "0" | "false" | "none" | "null"
    ))
}

fn eval_condition(expr: &str, vars: &HashMap<String, String>) -> bool {
    let tokens = tokenize_condition(expr);
    if tokens.is_empty() {
        return false;
    }
    let mut idx = 0usize;
    let mut result = match eval_condition_atom(&tokens, &mut idx, vars) {
        Some(v) => v,
        None => return false,
    };
    while idx < tokens.len() {
        let op = tokens[idx].to_ascii_lowercase();
        idx += 1;
        let rhs = match eval_condition_atom(&tokens, &mut idx, vars) {
            Some(v) => v,
            None => return false,
        };
        match op.as_str() {
            "and" => result = result && rhs,
            "or" => result = result || rhs,
            _ => return false,
        }
    }
    result
}

struct ScriptBuildContext {
    model_path: String,
    threshold: f32,
    flow_cfg: NetworkConfig,
    horizon_from_env: bool,
    friendbot_from_env: bool,
    runtime_settings: RuntimeSettings,
    models: HashMap<String, AIModel>,
    variables: HashMap<String, String>,
    plan: ActionPlan,
    intent_mode: bool,
    debug: bool,
    x402_state: X402State,
}

impl ScriptBuildContext {
    fn new(
        model_path: String,
        threshold: f32,
        flow_cfg: NetworkConfig,
        horizon_from_env: bool,
        friendbot_from_env: bool,
        debug: bool,
    ) -> Self {
        Self {
            model_path,
            threshold,
            flow_cfg,
            horizon_from_env,
            friendbot_from_env,
            runtime_settings: RuntimeSettings::default(),
            models: HashMap::new(),
            variables: HashMap::new(),
            plan: ActionPlan::default(),
            intent_mode: false,
            debug,
            x402_state: X402State::default(),
        }
    }

    fn apply_network(&mut self, network: &str) {
        self.flow_cfg.soroban_network = network.to_string();
        if !self.horizon_from_env {
            self.flow_cfg.horizon_url = default_horizon_url(network);
        }
        if !self.friendbot_from_env {
            self.flow_cfg.friendbot_url = default_friendbot_url(network);
        }
    }

    fn predict_current_model(&mut self, prompt: &str) -> Result<String> {
        let model = if let Some(model) = self.models.get(&self.model_path) {
            model.clone()
        } else {
            let loaded = AIModel::new(&self.model_path).with_context(|| {
                format!(
                    "failed to load model for set ... from AI: {}",
                    self.model_path
                )
            })?;
            self.models.insert(self.model_path.clone(), loaded.clone());
            loaded
        };
        model
            .predict(prompt)
            .context("set ... from AI prediction failed")
    }

    fn append_intent_prompt(&mut self, prompt: &str) -> Result<()> {
        self.intent_mode = true;
        let prompt_plan =
            build_plan_from_intent_prompt(prompt, &self.model_path, self.threshold, self.debug)?;
        merge_action_plans(&mut self.plan, prompt_plan);
        Ok(())
    }

    fn execute_statement(&mut self, line: &str) -> Result<()> {
        if let Some(new_model_path) = parse_ai_model_line(line) {
            self.model_path = new_model_path;
            return Ok(());
        }

        if let Some(network) = parse_network_line(line) {
            self.apply_network(&network);
            return Ok(());
        }

        if let Some(source) = parse_source_line(line) {
            self.flow_cfg.soroban_source = Some(source);
            return Ok(());
        }

        if let Some(alias) = parse_wallet_generate_line(line) {
            generate_wallet_alias(&self.flow_cfg, &alias)?;
            self.flow_cfg.soroban_source = Some(alias);
            return Ok(());
        }

        if let Some(alias) = parse_wallet_bootstrap_line(line) {
            let (_public_key, _fund_msg) = bootstrap_wallet_alias(&self.flow_cfg, &alias)?;
            self.flow_cfg.soroban_source = Some(alias);
            return Ok(());
        }

        if let Some(horizon_url) = parse_horizon_line(line) {
            self.flow_cfg.horizon_url = horizon_url;
            self.horizon_from_env = true;
            return Ok(());
        }

        if let Some(friendbot_url) = parse_friendbot_line(line) {
            self.flow_cfg.friendbot_url = friendbot_url;
            self.friendbot_from_env = true;
            return Ok(());
        }

        if let Some(cli_bin) = parse_stellar_cli_line(line) {
            self.flow_cfg.soroban_cli = cli_bin;
            return Ok(());
        }

        if let Some(simulate_flag) = parse_simulate_flag_line(line) {
            self.flow_cfg.soroban_simulate_args = parse_simulate_args(&simulate_flag);
            return Ok(());
        }

        if let Some(txrep_preview) = parse_txrep_line(line)? {
            self.flow_cfg.txrep_preview = txrep_preview;
            return Ok(());
        }

        if let Some(threshold) = parse_intent_threshold_line(line)? {
            self.threshold = threshold;
            return Ok(());
        }

        if let Some(assets) = parse_asset_allowlist_line(line) {
            self.runtime_settings.allowlist_assets = Some(assets);
            return Ok(());
        }

        if let Some(contracts) = parse_contract_allowlist_line(line) {
            self.runtime_settings.allowlist_contracts = Some(contracts);
            return Ok(());
        }

        if let Some(policy_path) = parse_contract_policy_line(line) {
            self.runtime_settings.contract_policy = policy_path;
            return Ok(());
        }

        if let Some(policy_dir) = parse_contract_policy_dir_line(line) {
            self.runtime_settings.contract_policy_dir = policy_dir;
            return Ok(());
        }

        if let Some(enforce) = parse_allowlist_enforce_line(line)? {
            self.runtime_settings.allowlist_enforce = Some(enforce);
            return Ok(());
        }

        if let Some(enforce) = parse_contract_policy_enforce_line(line)? {
            self.runtime_settings.contract_policy_enforce = Some(enforce);
            return Ok(());
        }

        if let Some(debug) = parse_debug_line(line)? {
            self.debug = debug;
            return Ok(());
        }

        if let Some(enabled) = parse_x402_line(line)? {
            self.runtime_settings.x402 = Some(enabled);
            return Ok(());
        }

        if let Some(payment) = parse_x402_request_line(line)? {
            if !x402_enabled(self.runtime_settings.x402) {
                return Err(anyhow!(
                    "x402 is disabled; enable with `x402` before x402.request"
                ));
            }
            let challenge_id = self.x402_state.create_challenge(payment);
            self.plan
                .warnings
                .push(format!("x402 challenge created: {challenge_id}"));
            return Ok(());
        }

        if let Some(requested_id) = parse_x402_finalize_line(line)? {
            if !x402_enabled(self.runtime_settings.x402) {
                return Err(anyhow!(
                    "x402 is disabled; enable with `x402` before x402.finalize"
                ));
            }
            let Some(challenge_id) = self.x402_state.resolve_challenge_id(&requested_id) else {
                return Err(anyhow!(
                    "x402 finalize failed: challenge `{requested_id}` not found"
                ));
            };
            let Some(challenge) = self.x402_state.challenges.get_mut(&challenge_id) else {
                return Err(anyhow!(
                    "x402 finalize failed: challenge `{challenge_id}` not found"
                ));
            };
            if challenge.finalized {
                return Err(anyhow!(
                    "x402 finalize blocked: challenge `{challenge_id}` already finalized (replay blocked)"
                ));
            }
            self.plan.actions.push(challenge.payment.clone());
            challenge.finalized = true;
            self.plan
                .warnings
                .push(format!("x402 finalize queued: challenge {challenge_id}"));
            return Ok(());
        }

        if let Some((name, prompt)) = parse_set_from_ai_assignment(line) {
            if is_intent_assignment_name(&name) {
                self.append_intent_prompt(&prompt)?;
            } else {
                let prediction = self
                    .predict_current_model(&prompt)
                    .with_context(|| format!("set_from_ai_failed: variable `{name}`"))?;
                self.variables.insert(name, prediction);
            }
            return Ok(());
        }

        if is_macro_from_ai_line(line) {
            return Err(anyhow!(
                "macro from AI is not supported in neurochain-stellar; use set stellar intent from AI: \"...\""
            ));
        }

        if let Some((name, value)) = parse_set_literal_assignment(line) {
            let resolved = self.variables.get(&value).cloned().unwrap_or(value);
            self.variables.insert(name, resolved);
            return Ok(());
        }

        if line_is_manual_action(line) {
            let manual_plan = parse_action_plan_from_nc(line);
            merge_action_plans(&mut self.plan, manual_plan);
            return Ok(());
        }

        if line.trim_start().starts_with("neuro ") {
            return Ok(());
        }

        let prompt = strip_wrapping_quotes(line);
        if !prompt.is_empty() {
            self.append_intent_prompt(&prompt)?;
        }
        Ok(())
    }
}

fn execute_script_block(
    lines: &[ScriptLine],
    idx: &mut usize,
    indent: usize,
    ctx: &mut ScriptBuildContext,
    execute: bool,
) -> Result<()> {
    while *idx < lines.len() {
        let line = &lines[*idx];
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(anyhow!(
                "unexpected indentation at line {}: {}",
                line.line_no,
                line.text
            ));
        }

        if parse_if_header(&line.text).is_some() {
            execute_if_chain(lines, idx, indent, ctx, execute)?;
            continue;
        }

        if parse_elif_header(&line.text).is_some() || is_else_header(&line.text) {
            return Err(anyhow!(
                "unexpected {} at line {}",
                line.text.split_whitespace().next().unwrap_or("branch"),
                line.line_no
            ));
        }

        if execute {
            ctx.execute_statement(&line.text)
                .with_context(|| format!("script execution failed at line {}", line.line_no))?;
        }
        *idx += 1;
    }
    Ok(())
}

fn execute_if_chain(
    lines: &[ScriptLine],
    idx: &mut usize,
    indent: usize,
    ctx: &mut ScriptBuildContext,
    execute: bool,
) -> Result<()> {
    let mut branch_taken = false;
    let mut first = true;

    loop {
        if *idx >= lines.len() {
            break;
        }
        let header = &lines[*idx];
        if header.indent != indent {
            break;
        }

        let (condition, is_else) = if first {
            let Some(cond) = parse_if_header(&header.text) else {
                return Err(anyhow!("expected if at line {}", header.line_no));
            };
            (cond, false)
        } else if let Some(cond) = parse_elif_header(&header.text) {
            (cond, false)
        } else if is_else_header(&header.text) {
            (String::new(), true)
        } else {
            break;
        };

        let should_execute_branch = if !execute || branch_taken {
            false
        } else if is_else {
            true
        } else {
            eval_condition(&condition, &ctx.variables)
        };

        *idx += 1;
        if *idx < lines.len() && lines[*idx].indent > indent {
            let body_indent = lines[*idx].indent;
            execute_script_block(lines, idx, body_indent, ctx, should_execute_branch)?;
        }

        if should_execute_branch {
            branch_taken = true;
        }

        if *idx >= lines.len() || lines[*idx].indent != indent {
            break;
        }

        if !(parse_elif_header(&lines[*idx].text).is_some() || is_else_header(&lines[*idx].text)) {
            break;
        }
        first = false;
    }

    Ok(())
}

fn build_plan_from_intent_prompt(
    prompt: &str,
    model_path: &str,
    threshold: f32,
    debug: bool,
) -> Result<ActionPlan> {
    let decision = classify_intent_stellar(prompt, model_path, threshold)?;
    intent_debug_log(
        debug,
        "classify",
        format!(
            "label={} score={:.4} threshold={:.2} downgraded_to_unknown={} model={} prompt=\"{}\"",
            decision.label.as_str(),
            decision.score,
            decision.threshold,
            decision.downgraded_to_unknown,
            model_path,
            prompt
        ),
    );

    let mut plan = build_intent_action_plan(prompt, &decision);
    plan.warnings
        .push(format!("intent_model: path={model_path}"));

    let action_kinds = if plan.actions.is_empty() {
        "(none)".to_string()
    } else {
        plan.actions
            .iter()
            .map(|a| a.kind())
            .collect::<Vec<_>>()
            .join(",")
    };
    let slot_errors = plan
        .warnings
        .iter()
        .filter(|w| w.starts_with("intent_error:"))
        .count();
    intent_debug_log(
        debug,
        "slot-parse",
        format!(
            "actions={} kinds={} intent_errors={} warnings={}",
            plan.actions.len(),
            action_kinds,
            slot_errors,
            plan.warnings.len()
        ),
    );

    Ok(plan)
}

fn predict_variable_from_model(
    models: &mut HashMap<String, AIModel>,
    model_path: &str,
    prompt: &str,
) -> Result<String> {
    let model = if let Some(model) = models.get(model_path) {
        model.clone()
    } else {
        let loaded = AIModel::new(model_path)
            .with_context(|| format!("failed to load model for set ... from AI: {model_path}"))?;
        models.insert(model_path.to_string(), loaded.clone());
        loaded
    };
    model
        .predict(prompt)
        .context("set ... from AI prediction failed")
}

fn resolve_threshold(override_value: Option<f32>) -> Result<f32> {
    if let Some(value) = override_value {
        return Ok(value);
    }
    Ok(intent_threshold_from_env()?.unwrap_or(DEFAULT_INTENT_STELLAR_THRESHOLD))
}

fn merge_action_plans(target: &mut ActionPlan, mut other: ActionPlan) {
    target.actions.append(&mut other.actions);
    target.warnings.append(&mut other.warnings);
}

fn build_plan_from_script(
    script: &str,
    source_path: &str,
    initial_model: Option<String>,
    initial_threshold: Option<f32>,
    debug: bool,
) -> Result<(ActionPlan, NetworkConfig, bool, RuntimeSettings)> {
    let model_path = initial_model.unwrap_or_else(resolve_intent_model_path);
    let threshold = resolve_threshold(initial_threshold)?;
    let horizon_from_env = env::var("NC_STELLAR_HORIZON_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let friendbot_from_env = env::var("NC_FRIENDBOT_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let flow_cfg = load_network_config();
    let mut ctx = ScriptBuildContext::new(
        model_path,
        threshold,
        flow_cfg,
        horizon_from_env,
        friendbot_from_env,
        debug,
    );
    let lines = collect_script_lines(script);
    let mut idx = 0usize;
    execute_script_block(&lines, &mut idx, 0, &mut ctx, true)?;

    if idx < lines.len() {
        return Err(anyhow!(
            "unparsed script content at line {}",
            lines[idx].line_no
        ));
    }

    if ctx.plan.source.is_none() {
        ctx.plan.source = Some(source_path.to_string());
    }

    Ok((
        ctx.plan,
        ctx.flow_cfg,
        ctx.intent_mode,
        ctx.runtime_settings,
    ))
}

fn execute_plan(
    mut plan: ActionPlan,
    flow: bool,
    auto_yes: bool,
    intent_mode: bool,
    cfg_override: Option<&NetworkConfig>,
    runtime_settings: Option<&RuntimeSettings>,
    debug: bool,
) -> i32 {
    let runtime_settings = runtime_settings.cloned().unwrap_or_default();
    let policies = load_contract_policies(Some(&runtime_settings));
    if intent_mode {
        let (typed_v2_converted, typed_v2_normalized_args) =
            apply_policy_typed_templates_v2(&mut plan, &policies);
        intent_debug_log(
            debug,
            "typed-template-v2",
            format!(
                "policy_slot_type_converted={typed_v2_converted} normalized_args={typed_v2_normalized_args}"
            ),
        );
    }
    let allowlist = runtime_settings.allowlist();
    let violations = validate_plan(&plan, &allowlist);
    let allowlist_is_enforced = allowlist_enforced(runtime_settings.allowlist_enforce);
    intent_debug_log(
        debug,
        "guardrails",
        format!(
            "allowlist_violations={} enforce={}",
            violations.len(),
            allowlist_is_enforced
        ),
    );
    if !violations.is_empty() {
        for violation in &violations {
            plan.warnings.push(format!(
                "allowlist warning: #{} {} ({})",
                violation.index, violation.action, violation.reason
            ));
        }
        if allowlist_is_enforced {
            eprintln!("Allowlist violations (enforced):");
            for violation in &violations {
                eprintln!(
                    "- #{} {}: {}",
                    violation.index, violation.action, violation.reason
                );
            }
            eprintln!(
                "Set NC_ALLOWLIST_ENFORCE=0 (or unset), or use allowlist_enforce: off in REPL/.nc."
            );
            intent_debug_log(debug, "guardrails", "allowlist blocked execution (exit=3)");
            return 3;
        }
        eprintln!("Allowlist warnings (stub, not enforced):");
        for violation in &violations {
            eprintln!(
                "- #{} {}: {}",
                violation.index, violation.action, violation.reason
            );
        }
    }

    let (policy_warnings, policy_errors) = validate_contract_policies(&plan, &policies);
    let policy_is_enforced = policy_enforced(runtime_settings.contract_policy_enforce);
    intent_debug_log(
        debug,
        "guardrails",
        format!(
            "policy_warnings={} policy_errors={} enforce={}",
            policy_warnings.len(),
            policy_errors.len(),
            policy_is_enforced
        ),
    );
    for warning in &policy_warnings {
        plan.warnings.push(format!("policy warning: {warning}"));
    }
    if !policy_errors.is_empty() {
        if policy_is_enforced {
            eprintln!("Contract policy violations (enforced):");
            for err in &policy_errors {
                eprintln!("- {err}");
            }
            eprintln!(
                "Set NC_CONTRACT_POLICY_ENFORCE=0 (or unset), or use contract_policy_enforce: off in REPL/.nc."
            );
            intent_debug_log(
                debug,
                "guardrails",
                "contract policy blocked execution (exit=4)",
            );
            return 4;
        }
        eprintln!("Contract policy warnings (not enforced):");
        for err in &policy_errors {
            eprintln!("- {err}");
            plan.warnings.push(format!("policy error: {err}"));
        }
    }

    intent_debug_log(
        debug,
        "plan",
        format!(
            "actions={} warnings={}",
            plan.actions.len(),
            plan.warnings.len()
        ),
    );

    match serde_json::to_string_pretty(&plan) {
        Ok(json) => println!("{json}"),
        Err(err) => {
            eprintln!("Error serializing action plan: {err}");
            return 1;
        }
    }

    if flow {
        intent_debug_log(debug, "flow", "enabled");
        if intent_mode && has_intent_blocking_issue(&plan) {
            eprintln!("Intent safety guard blocked flow. simulate/submit skipped.");
            print_intent_block_reasons(&plan);
            intent_debug_log(
                debug,
                "flow",
                "intent safety guard blocked execution (exit=5)",
            );
            return 5;
        }
        let cfg = cfg_override.cloned().unwrap_or_else(load_network_config);
        let preview = simulate_plan(&plan, &cfg);
        intent_debug_log(
            debug,
            "flow",
            format!(
                "simulate effects={} warnings={}",
                preview.effects.len(),
                preview.warnings.len()
            ),
        );
        print_preview(&preview);
        if confirm_submit(auto_yes) {
            intent_debug_log(debug, "flow", "submit confirmed");
            let outputs = submit_plan(&plan, &cfg);
            if outputs.is_empty() {
                eprintln!("Submit: no actions executed.");
                intent_debug_log(debug, "flow", "submit produced no output lines");
            } else {
                eprintln!("Submit results:");
                intent_debug_log(
                    debug,
                    "flow",
                    format!("submit output_lines={}", outputs.len()),
                );
                for line in outputs {
                    eprintln!("  - {line}");
                }
            }
        } else {
            eprintln!("Submit aborted by user.");
            intent_debug_log(debug, "flow", "submit aborted by user");
        }
    } else {
        intent_debug_log(debug, "flow", "disabled (plan-only)");
    }

    0
}

fn line_is_manual_action(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("stellar.")
        || trimmed.starts_with("soroban.")
        || trimmed.starts_with("action stellar.")
        || trimmed.starts_with("action soroban.")
}

fn line_is_comment_or_empty(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("//")
        || trimmed.starts_with("- ")
}

fn print_repl_config(
    model_path: &str,
    threshold: f32,
    debug: bool,
    cfg: &NetworkConfig,
    runtime: &RuntimeSettings,
) {
    println!("Current REPL config:");
    println!("- model: {model_path}");
    println!("- intent_threshold: {threshold:.2}");
    println!("- intent_debug: {}", if debug { "on" } else { "off" });
    println!("- network: {}", cfg.soroban_network);
    println!(
        "- wallet/source: {}",
        cfg.soroban_source.as_deref().unwrap_or("(not set)")
    );
    println!("- horizon: {}", cfg.horizon_url);
    println!(
        "- friendbot: {}",
        cfg.friendbot_url.as_deref().unwrap_or("(disabled)")
    );
    println!("- stellar_cli: {}", cfg.soroban_cli);
    println!(
        "- simulate_flag: {}",
        if cfg.soroban_simulate_args.is_empty() {
            "(empty)".to_string()
        } else {
            cfg.soroban_simulate_args.join(" ")
        }
    );
    println!(
        "- txrep_preview: {}",
        if cfg.txrep_preview { "on" } else { "off" }
    );
    println!(
        "- x402: {}",
        if x402_enabled(runtime.x402) {
            "on"
        } else {
            "off"
        }
    );
    println!(
        "- asset_allowlist: {}",
        runtime
            .allowlist_assets
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("(env/default)")
    );
    println!(
        "- soroban_allowlist: {}",
        runtime
            .allowlist_contracts
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("(env/default)")
    );
    println!(
        "- allowlist_enforce: {}",
        if allowlist_enforced(runtime.allowlist_enforce) {
            "on"
        } else {
            "off"
        }
    );
    println!(
        "- contract_policy: {}",
        runtime
            .contract_policy
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("(env/default)")
    );
    println!(
        "- contract_policy_dir: {}",
        runtime
            .contract_policy_dir
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("(env/default: contracts)")
    );
    println!(
        "- contract_policy_enforce: {}",
        if policy_enforced(runtime.contract_policy_enforce) {
            "on"
        } else {
            "off"
        }
    );
}

fn repl_enabled_toggles(cfg: &NetworkConfig, runtime: &RuntimeSettings, debug: bool) -> String {
    let mut toggles: Vec<&str> = Vec::new();
    if cfg.txrep_preview {
        toggles.push("txrep");
    }
    if allowlist_enforced(runtime.allowlist_enforce) {
        toggles.push("allowlist_enforce");
    }
    if policy_enforced(runtime.contract_policy_enforce) {
        toggles.push("contract_policy_enforce");
    }
    if x402_enabled(runtime.x402) {
        toggles.push("x402");
    }
    if debug {
        toggles.push("debug");
    }
    if toggles.is_empty() {
        "(none)".to_string()
    } else {
        toggles.join(", ")
    }
}

fn print_repl_setup(
    model_path: &str,
    threshold: f32,
    debug: bool,
    cfg: &NetworkConfig,
    runtime: &RuntimeSettings,
    flow: bool,
) {
    println!("Current REPL setup:");
    println!("- model: {model_path}");
    println!("- intent_threshold: {threshold:.2}");
    println!("- intent_debug: {}", if debug { "on" } else { "off" });
    println!("- network: {}", cfg.soroban_network);
    println!(
        "- wallet/source: {}",
        cfg.soroban_source.as_deref().unwrap_or("(not set)")
    );
    println!("- flow_mode: {}", if flow { "on" } else { "off" });
    println!(
        "- asset_allowlist: {}",
        runtime
            .allowlist_assets
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("(env/default)")
    );
    println!(
        "- enabled_toggles: {}",
        repl_enabled_toggles(cfg, runtime, debug)
    );
}

fn print_repl_active_settings(
    cfg: &NetworkConfig,
    runtime: &RuntimeSettings,
    debug: bool,
    include_asset_allowlist: bool,
) -> bool {
    let mut any = false;
    if include_asset_allowlist {
        if let Some(assets) = runtime
            .allowlist_assets
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            println!("- asset_allowlist: {assets}");
            any = true;
        }
    }
    if let Some(contracts) = runtime
        .allowlist_contracts
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        println!("- soroban_allowlist: {contracts}");
        any = true;
    }
    if cfg.txrep_preview {
        println!("- txrep");
        any = true;
    }
    if allowlist_enforced(runtime.allowlist_enforce) {
        println!("- allowlist_enforce");
        any = true;
    }
    if let Some(policy_path) = runtime
        .contract_policy
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        println!("- contract_policy: {policy_path}");
        any = true;
    }
    if let Some(policy_dir) = runtime
        .contract_policy_dir
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        println!("- contract_policy_dir: {policy_dir}");
        any = true;
    }
    if policy_enforced(runtime.contract_policy_enforce) {
        println!("- contract_policy_enforce");
        any = true;
    }
    if x402_enabled(runtime.x402) {
        println!("- x402");
        any = true;
    }
    if debug {
        println!("- debug");
        any = true;
    }
    any
}

fn print_repl_current_asset_allowlist(runtime: &RuntimeSettings) -> bool {
    if let Some(assets) = runtime
        .allowlist_assets
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        println!("Current asset_allowlist: {assets}");
        return true;
    }
    false
}

fn print_repl_help_quick(_cfg: &NetworkConfig, _runtime: &RuntimeSettings, _debug: bool) {
    const HELP_COL_WIDTH: usize = 58;
    println!("Stellar REPL quick start:");
    let quick_rows = [
        ("AI: \"models/intent_stellar/model.onnx\"", ""),
        ("network: testnet", ""),
        ("wallet: nc-testnet", ""),
        (
            "wallet_generate: demo-alias",
            "(create key alias + set wallet)",
        ),
        (
            "wallet_bootstrap: demo-alias",
            "(generate key + friendbot fund + set wallet)",
        ),
        (
            "asset_allowlist: XLM,USDC:GISSUER",
            "(change allowed assets; default startup is XLM)",
        ),
        ("txrep", "(optional preview on)"),
        ("x402", "(optional x402-lite mode on)"),
        ("allowlist_enforce", "(optional hard-fail on allowlist)"),
        ("contract_policy: <path>", "(optional policy file)"),
        ("contract_policy_enforce", "(optional hard-fail on policy)"),
        (
            "x402.request to=\"...\" amount=\"...\" asset_code=\"XLM\"",
            "(create x402-lite challenge)",
        ),
        (
            "x402.finalize challenge_id=\"last\"",
            "(finalize latest challenge -> typed payment plan)",
        ),
        ("debug", "(optional intent trace on)"),
        (
            "set <var> from AI: \"...\"",
            "(store model prediction to variable)",
        ),
        (
            "set stellar intent from AI: \"...\"",
            "(classify prompt -> ActionPlan)",
        ),
        (
            "soroban.contract.deploy alias=\"...\" wasm=\"...\"",
            "(manual deploy action)",
        ),
        ("help all", "(show every command)"),
        ("help dsl", "(show normal NeuroChain DSL help)"),
        ("show setup", "(print active setup)"),
        ("show config", "(print active config)"),
        ("setup testnet", "(set network+horizon+friendbot baseline)"),
        ("exit", ""),
    ];
    for (command, desc) in quick_rows {
        if desc.is_empty() {
            println!("- {command}");
        } else {
            println!("- {:<HELP_COL_WIDTH$} {}", command, desc);
        }
    }
    println!("- Toggle commands are listed in `help all` under Toggles (on/off).");
    println!(
        "- REPL startup default asset_allowlist is XLM; change it with `asset_allowlist: ...`."
    );
    println!("- restart with --no-flow if you want plan-only REPL");
}

fn print_repl_help_section(title: &str, rows: &[(&str, &str)]) {
    const HELP_COL_WIDTH: usize = 58;
    println!("{title}:");
    for (command, description) in rows {
        println!("- {:<HELP_COL_WIDTH$} {}", command, description);
    }
    println!();
}

fn divider_line() -> String {
    let columns = env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(96)
        .clamp(48, 160);
    "_".repeat(columns.saturating_sub(18))
}

fn print_repl_divider() {
    // Keep a small right margin so the divider does not feel edge-to-edge.
    let line = divider_line();
    if env::var("NO_COLOR").is_ok() {
        println!("{line}");
    } else {
        println!("\x1b[94m{line}\x1b[0m");
    }
}

fn print_script_divider_stderr() {
    let line = divider_line();
    if env::var("NO_COLOR").is_ok() {
        eprintln!("{line}");
    } else {
        eprintln!("\x1b[94m{line}\x1b[0m");
    }
}

fn print_script_setup(
    path: &str,
    cfg: &NetworkConfig,
    runtime: Option<&RuntimeSettings>,
    flow: bool,
    debug: bool,
) {
    eprintln!("Script execution setup:");
    print_script_divider_stderr();
    eprintln!("- source: {path}");
    eprintln!("- network: {}", cfg.soroban_network);
    eprintln!(
        "- wallet/source: {}",
        cfg.soroban_source.as_deref().unwrap_or("(not set)")
    );
    eprintln!("- flow_mode: {}", if flow { "on" } else { "off" });
    eprintln!("- intent_debug: {}", if debug { "on" } else { "off" });
    eprintln!(
        "- txrep_preview: {}",
        if cfg.txrep_preview { "on" } else { "off" }
    );
    if let Some(runtime) = runtime {
        if let Some(assets) = runtime
            .allowlist_assets
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            eprintln!("- asset_allowlist: {assets}");
        }
        if let Some(contracts) = runtime
            .allowlist_contracts
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            eprintln!("- soroban_allowlist: {contracts}");
        }
        if let Some(policy_path) = runtime
            .contract_policy
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            eprintln!("- contract_policy: {policy_path}");
        }
        if let Some(policy_dir) = runtime
            .contract_policy_dir
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            eprintln!("- contract_policy_dir: {policy_dir}");
        }
        eprintln!(
            "- allowlist_enforce: {}",
            if allowlist_enforced(runtime.allowlist_enforce) {
                "on"
            } else {
                "off"
            }
        );
        eprintln!(
            "- contract_policy_enforce: {}",
            if policy_enforced(runtime.contract_policy_enforce) {
                "on"
            } else {
                "off"
            }
        );
        eprintln!(
            "- x402: {}",
            if x402_enabled(runtime.x402) {
                "on"
            } else {
                "off"
            }
        );
    }
    print_script_divider_stderr();
}

fn print_repl_hint_line() {
    if env::var("NO_COLOR").is_ok() {
        println!("Type `help` for quick start, `help all` for full command list, `exit` to quit.");
    } else {
        let green = "\x1b[92m";
        let red = "\x1b[91m";
        let reset = "\x1b[0m";
        println!(
            "Type {green}help{reset} for quick start, {green}help all{reset} for full command list, {red}exit{reset} to quit."
        );
    }
}

fn print_repl_help_all() {
    println!("Stellar REPL commands (all):");
    println!();

    let core_setup = [
        ("AI: \"path\"", "set intent model path"),
        ("intent_threshold: <f32>", "set intent confidence threshold"),
        (
            "network: testnet|mainnet|public",
            "set active network for flow",
        ),
        (
            "wallet: <stellar-key-alias>",
            "set active source wallet alias",
        ),
        (
            "wallet_generate: <alias>",
            "generate a local stellar key alias",
        ),
        (
            "wallet_bootstrap: <alias>",
            "generate key alias and friendbot-fund it",
        ),
        ("horizon: https://...", "set Horizon URL override"),
        (
            "friendbot: https://...|off",
            "set Friendbot URL or disable it",
        ),
        ("stellar_cli: <bin>", "set stellar CLI binary path/name"),
        ("simulate_flag: \"--send no\"", "set soroban simulate flag"),
        (
            "asset_allowlist: XLM,USDC:G...",
            "set NC_ASSET_ALLOWLIST equivalent",
        ),
        (
            "soroban_allowlist: C1:transfer,C2",
            "set NC_SOROBAN_ALLOWLIST equivalent",
        ),
        (
            "contract_policy: <path>",
            "set NC_CONTRACT_POLICY equivalent",
        ),
        (
            "contract_policy_dir: <dir>",
            "set NC_CONTRACT_POLICY_DIR equivalent",
        ),
    ];
    print_repl_help_section("Core setup (value required)", &core_setup);
    println!(
        "Note: REPL startup default is `asset_allowlist: XLM`; override with `asset_allowlist: ...`."
    );
    println!();

    let toggles = [
        ("txrep", "enable txrep preview in flow"),
        ("txrep off", "disable txrep preview in flow"),
        ("x402", "enable x402-lite flow commands"),
        ("x402 off", "disable x402-lite flow commands"),
        ("allowlist_enforce", "enable allowlist enforce"),
        ("allowlist_enforce off", "disable allowlist enforce"),
        ("contract_policy_enforce", "enable contract policy enforce"),
        (
            "contract_policy_enforce off",
            "disable contract policy enforce",
        ),
        ("debug", "enable intent pipeline trace"),
        ("debug off", "disable intent pipeline trace"),
    ];
    print_repl_help_section("Toggles (on/off)", &toggles);

    let prompt_actions = [
        (
            "set <var> from AI: \"...\"",
            "predict with active model -> store variable",
        ),
        (
            "set stellar intent from AI: \"...\"",
            "classify prompt -> ActionPlan",
        ),
        (
            "soroban.contract.deploy alias=\"...\" wasm=\"...\"",
            "manual deploy action",
        ),
        (
            "x402.request to=\"...\" amount=\"...\" asset_code=\"XLM\"",
            "create x402-lite payment challenge",
        ),
        (
            "x402.finalize challenge_id=\"last\"",
            "finalize challenge -> execute typed stellar_payment",
        ),
        (
            "macro from AI: \"...\"",
            "not supported here (use set stellar intent from AI)",
        ),
        ("plain text prompt", "classify prompt -> ActionPlan"),
        ("stellar.* / soroban.* lines", "manual action-plan mode"),
    ];
    print_repl_help_section("Prompt/Action commands", &prompt_actions);

    let utility = [
        ("help", "show quick start"),
        ("help all", "show every command"),
        ("help dsl", "show normal NeuroChain DSL language help"),
        ("show setup", "print active setup"),
        ("show config", "print active config"),
        ("setup testnet", "set network+horizon+friendbot baseline"),
        ("exit", "leave REPL"),
    ];
    print_repl_help_section("Utility commands", &utility);
}

fn run_repl(
    flow: bool,
    auto_yes: bool,
    initial_model: Option<String>,
    initial_threshold: Option<f32>,
    initial_debug: bool,
) -> i32 {
    let mut model_path = initial_model.unwrap_or_else(resolve_intent_model_path);
    let mut models: HashMap<String, AIModel> = HashMap::new();
    let mut variables: HashMap<String, String> = HashMap::new();
    let mut threshold = match resolve_threshold(initial_threshold) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Error: {err}");
            return 2;
        }
    };
    let mut debug = resolve_intent_debug(initial_debug);
    let mut horizon_from_env = env::var("NC_STELLAR_HORIZON_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let mut friendbot_from_env = env::var("NC_FRIENDBOT_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let mut flow_cfg = load_network_config();
    // REPL is wallet-explicit: do not preload wallet/source from env on startup.
    flow_cfg.soroban_source = None;
    let mut runtime_settings = runtime_settings_from_env();
    if runtime_settings.allowlist_assets.is_none() {
        runtime_settings.allowlist_assets = Some("XLM".to_string());
    }
    let mut x402_state = X402State::default();

    println!("NeuroChain Stellar REPL (intent -> action).");
    print_repl_divider();
    println!("Current model: {model_path}");
    println!("Current threshold: {threshold:.2}");
    println!("Current intent debug: {}", if debug { "on" } else { "off" });
    println!("Current network: {}", flow_cfg.soroban_network);
    println!(
        "Current wallet/source: {}",
        flow_cfg.soroban_source.as_deref().unwrap_or("(not set)")
    );
    let _ = print_repl_current_asset_allowlist(&runtime_settings);
    println!(
        "Flow mode: {}",
        if flow {
            "enabled (default REPL)"
        } else {
            "disabled"
        }
    );
    if print_repl_active_settings(&flow_cfg, &runtime_settings, debug, false) {
        println!();
    }
    print_repl_divider();
    print_repl_hint_line();
    print_repl_divider();

    loop {
        println!("Enter Stellar prompt/code (finish with an empty line):");
        let mut block = String::new();
        loop {
            print!("... ");
            let _ = io::stdout().flush();
            let mut line = String::new();
            if io::stdin().read_line(&mut line).is_err() {
                eprintln!("stdin read failed");
                return 1;
            }
            if line.trim().is_empty() {
                break;
            }
            block.push_str(&line);
        }

        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lowered = trimmed.to_ascii_lowercase();
        match lowered.as_str() {
            "exit" | "quit" => {
                println!("Exiting...");
                return 0;
            }
            "help" => {
                print_repl_help_quick(&flow_cfg, &runtime_settings, debug);
                continue;
            }
            "help all" => {
                print_repl_help_all();
                continue;
            }
            "help dsl" => {
                println!("{}", neurochain_language_help());
                continue;
            }
            "show setup" => {
                print_repl_setup(
                    &model_path,
                    threshold,
                    debug,
                    &flow_cfg,
                    &runtime_settings,
                    flow,
                );
                continue;
            }
            "show config" => {
                print_repl_config(&model_path, threshold, debug, &flow_cfg, &runtime_settings);
                continue;
            }
            "setup testnet" => {
                flow_cfg.soroban_network = "testnet".to_string();
                flow_cfg.horizon_url = default_horizon_url("testnet");
                flow_cfg.friendbot_url = default_friendbot_url("testnet");
                horizon_from_env = false;
                friendbot_from_env = false;
                println!("Applied testnet baseline (network+horizon+friendbot).");
                println!("- network: {}", flow_cfg.soroban_network);
                println!("- horizon: {}", flow_cfg.horizon_url);
                println!(
                    "- friendbot: {}",
                    flow_cfg.friendbot_url.as_deref().unwrap_or("(disabled)")
                );
                println!("Tip: run `show config` for full details.");
                continue;
            }
            _ => {}
        }

        let lines: Vec<String> = trimmed
            .lines()
            .map(strip_inline_comment_outside_quotes)
            .filter(|l| !line_is_comment_or_empty(l))
            .collect();
        let all_manual_actions =
            !lines.is_empty() && lines.iter().all(|l| line_is_manual_action(l));
        if all_manual_actions {
            let manual_src = lines.join("\n");
            let mut plan = parse_action_plan_from_nc(&manual_src);
            if plan.source.is_none() {
                plan.source = Some("repl.manual".to_string());
            }
            let code = execute_plan(
                plan,
                flow,
                auto_yes,
                false,
                Some(&flow_cfg),
                Some(&runtime_settings),
                debug,
            );
            if code != 0 {
                eprintln!("repl step returned code {code}");
            }
            continue;
        }

        for line in lines {
            if let Some(new_model_path) = parse_ai_model_line(&line) {
                model_path = new_model_path;
                println!("Intent model path set to: {model_path}");
                continue;
            }

            if let Some(network) = parse_network_line(&line) {
                flow_cfg.soroban_network = network.to_string();
                if !horizon_from_env {
                    flow_cfg.horizon_url = default_horizon_url(&network);
                }
                if !friendbot_from_env {
                    flow_cfg.friendbot_url = default_friendbot_url(&network);
                }
                println!("Network set to: {}", flow_cfg.soroban_network);
                println!("Horizon URL: {}", flow_cfg.horizon_url);
                println!(
                    "Friendbot: {}",
                    flow_cfg.friendbot_url.as_deref().unwrap_or("(disabled)")
                );
                continue;
            }

            if let Some(source) = parse_source_line(&line) {
                flow_cfg.soroban_source = Some(source.to_string());
                println!(
                    "Wallet/source set to: {}",
                    flow_cfg.soroban_source.as_deref().unwrap_or("")
                );
                continue;
            }

            if let Some(alias) = parse_wallet_generate_line(&line) {
                match generate_wallet_alias(&flow_cfg, &alias) {
                    Ok(public_key) => {
                        flow_cfg.soroban_source = Some(alias.to_string());
                        println!("Wallet key alias generated: {alias}");
                        println!("Public key/address: {public_key}");
                        println!(
                            "Wallet/source set to: {}",
                            flow_cfg.soroban_source.as_deref().unwrap_or("")
                        );
                    }
                    Err(err) => eprintln!("wallet_generate failed for `{alias}`: {err}"),
                }
                continue;
            }

            if let Some(alias) = parse_wallet_bootstrap_line(&line) {
                match bootstrap_wallet_alias(&flow_cfg, &alias) {
                    Ok((public_key, fund_msg)) => {
                        flow_cfg.soroban_source = Some(alias.to_string());
                        println!("Wallet key alias generated: {alias}");
                        println!("Public key/address: {public_key}");
                        println!("Friendbot: {fund_msg}");
                        println!(
                            "Wallet/source set to: {}",
                            flow_cfg.soroban_source.as_deref().unwrap_or("")
                        );
                    }
                    Err(err) => eprintln!("wallet_bootstrap failed for `{alias}`: {err}"),
                }
                continue;
            }

            if let Some(horizon_url) = parse_horizon_line(&line) {
                flow_cfg.horizon_url = horizon_url;
                horizon_from_env = true;
                println!("Horizon URL set to: {}", flow_cfg.horizon_url);
                continue;
            }

            if let Some(friendbot_url) = parse_friendbot_line(&line) {
                flow_cfg.friendbot_url = friendbot_url;
                friendbot_from_env = true;
                println!(
                    "Friendbot set to: {}",
                    flow_cfg.friendbot_url.as_deref().unwrap_or("(disabled)")
                );
                continue;
            }

            if let Some(cli_bin) = parse_stellar_cli_line(&line) {
                flow_cfg.soroban_cli = cli_bin;
                println!("Stellar CLI binary set to: {}", flow_cfg.soroban_cli);
                continue;
            }

            if let Some(simulate_flag) = parse_simulate_flag_line(&line) {
                flow_cfg.soroban_simulate_args = parse_simulate_args(&simulate_flag);
                println!(
                    "Soroban simulate flag set to: {}",
                    if simulate_flag.trim().is_empty() {
                        "(empty)"
                    } else {
                        simulate_flag.trim()
                    }
                );
                continue;
            }

            match parse_txrep_line(&line) {
                Ok(Some(enabled)) => {
                    flow_cfg.txrep_preview = enabled;
                    println!(
                        "Txrep preview: {}",
                        if enabled { "enabled" } else { "disabled" }
                    );
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            }

            match parse_intent_threshold_line(&line) {
                Ok(Some(value)) => {
                    threshold = value;
                    println!("Intent threshold set to: {threshold:.2}");
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            }

            if let Some(assets) = parse_asset_allowlist_line(&line) {
                runtime_settings.allowlist_assets = Some(assets.clone());
                println!("Asset allowlist set to: {assets}");
                continue;
            }

            if let Some(contracts) = parse_contract_allowlist_line(&line) {
                runtime_settings.allowlist_contracts = Some(contracts.clone());
                println!("Soroban allowlist set to: {contracts}");
                continue;
            }

            if let Some(policy_path) = parse_contract_policy_line(&line) {
                runtime_settings.contract_policy = policy_path.clone();
                println!(
                    "Contract policy file: {}",
                    policy_path.as_deref().unwrap_or("(disabled)")
                );
                continue;
            }

            if let Some(policy_dir) = parse_contract_policy_dir_line(&line) {
                runtime_settings.contract_policy_dir = policy_dir.clone();
                println!(
                    "Contract policy dir: {}",
                    policy_dir.as_deref().unwrap_or("(disabled)")
                );
                continue;
            }

            match parse_allowlist_enforce_line(&line) {
                Ok(Some(enabled)) => {
                    runtime_settings.allowlist_enforce = Some(enabled);
                    println!(
                        "Allowlist enforce: {}",
                        if enabled { "enabled" } else { "disabled" }
                    );
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            }

            match parse_contract_policy_enforce_line(&line) {
                Ok(Some(enabled)) => {
                    runtime_settings.contract_policy_enforce = Some(enabled);
                    println!(
                        "Contract policy enforce: {}",
                        if enabled { "enabled" } else { "disabled" }
                    );
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            }

            match parse_debug_line(&line) {
                Ok(Some(enabled)) => {
                    debug = enabled;
                    println!(
                        "Intent debug trace: {}",
                        if enabled { "enabled" } else { "disabled" }
                    );
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            }

            match parse_x402_line(&line) {
                Ok(Some(enabled)) => {
                    runtime_settings.x402 = Some(enabled);
                    println!(
                        "x402 mode: {}",
                        if enabled { "enabled" } else { "disabled" }
                    );
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            }

            match parse_x402_request_line(&line) {
                Ok(Some(payment)) => {
                    if !x402_enabled(runtime_settings.x402) {
                        eprintln!("x402 is disabled; run `x402` first.");
                        continue;
                    }
                    let payment_desc = describe_stellar_payment(&payment)
                        .unwrap_or_else(|| "stellar payment".to_string());
                    let challenge_id = x402_state.create_challenge(payment);
                    println!("x402 challenge created: {challenge_id}");
                    println!("- payment: {payment_desc}");
                    println!("- finalize with: x402.finalize challenge_id=\"{challenge_id}\"");
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            }

            match parse_x402_finalize_line(&line) {
                Ok(Some(requested_id)) => {
                    if !x402_enabled(runtime_settings.x402) {
                        eprintln!("x402 is disabled; run `x402` first.");
                        continue;
                    }
                    let Some(challenge_id) = x402_state.resolve_challenge_id(&requested_id) else {
                        eprintln!("x402 finalize failed: challenge `{requested_id}` not found");
                        continue;
                    };
                    let Some(challenge) = x402_state.challenges.get(&challenge_id) else {
                        eprintln!("x402 finalize failed: challenge `{challenge_id}` not found");
                        continue;
                    };
                    if challenge.finalized {
                        eprintln!(
                            "x402 finalize blocked: challenge `{challenge_id}` already finalized (replay blocked)"
                        );
                        continue;
                    }
                    let plan = ActionPlan {
                        source: Some("repl.x402.finalize".to_string()),
                        actions: vec![challenge.payment.clone()],
                        ..ActionPlan::default()
                    };
                    println!("x402 finalize: challenge `{challenge_id}`");
                    let code = execute_plan(
                        plan,
                        flow,
                        auto_yes,
                        false,
                        Some(&flow_cfg),
                        Some(&runtime_settings),
                        debug,
                    );
                    if code != 0 {
                        eprintln!("repl step returned code {code}");
                    } else if let Some(challenge_mut) = x402_state.challenges.get_mut(&challenge_id)
                    {
                        challenge_mut.finalized = true;
                    }
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            }

            if let Some((name, prompt)) = parse_set_from_ai_assignment(&line) {
                if is_intent_assignment_name(&name) {
                    match build_plan_from_intent_prompt(&prompt, &model_path, threshold, debug) {
                        Ok(plan) => {
                            let code = execute_plan(
                                plan,
                                flow,
                                auto_yes,
                                true,
                                Some(&flow_cfg),
                                Some(&runtime_settings),
                                debug,
                            );
                            if code != 0 {
                                eprintln!("repl step returned code {code}");
                            }
                        }
                        Err(err) => eprintln!("intent error: {err}"),
                    }
                } else {
                    match predict_variable_from_model(&mut models, &model_path, &prompt) {
                        Ok(prediction) => {
                            variables.insert(name.clone(), prediction.clone());
                            println!("Variable {name} set from AI: {prediction}");
                        }
                        Err(err) => {
                            eprintln!("set_from_ai_failed {name}: {err}");
                        }
                    }
                }
                continue;
            }

            if is_macro_from_ai_line(&line) {
                eprintln!(
                    "macro from AI is not supported in neurochain-stellar; use set stellar intent from AI: \"...\""
                );
                continue;
            }

            if let Some(msg) = line.trim().strip_prefix("neuro ") {
                println!("{}", strip_wrapping_quotes(msg));
                continue;
            }

            let prompt = strip_wrapping_quotes(&line);
            match build_plan_from_intent_prompt(&prompt, &model_path, threshold, debug) {
                Ok(plan) => {
                    let code = execute_plan(
                        plan,
                        flow,
                        auto_yes,
                        true,
                        Some(&flow_cfg),
                        Some(&runtime_settings),
                        debug,
                    );
                    if code != 0 {
                        eprintln!("repl step returned code {code}");
                    }
                }
                Err(err) => eprintln!("intent error: {err}"),
            }
        }
    }
}

fn main() {
    banner::print_banner_stderr();
    let args: Vec<String> = env::args().collect();
    let cli = match parse_cli_args(&args) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("Error: {err}");
            print_usage();
            std::process::exit(2);
        }
    };

    let debug = resolve_intent_debug(cli.debug);

    if cli.repl {
        let code = run_repl(
            cli.flow,
            cli.auto_yes,
            cli.intent_model,
            cli.intent_threshold,
            debug,
        );
        if code != 0 {
            std::process::exit(code);
        }
        return;
    }

    let mut cfg_override: Option<NetworkConfig> = None;
    let mut runtime_override: Option<RuntimeSettings> = None;
    let mut intent_mode = false;
    let mut script_setup_path: Option<String> = None;
    let plan: ActionPlan = if let Some(prompt) = cli.intent_text {
        intent_mode = true;
        let threshold = match resolve_threshold(cli.intent_threshold) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("Error: {err}");
                std::process::exit(2);
            }
        };
        let model_path = cli.intent_model.unwrap_or_else(resolve_intent_model_path);
        match build_plan_from_intent_prompt(&prompt, &model_path, threshold, debug) {
            Ok(plan) => plan,
            Err(err) => {
                eprintln!("Error: {err}");
                std::process::exit(1);
            }
        }
    } else {
        let path = cli.path.expect("path must exist when not in intent mode");
        let input = match fs::read_to_string(path.clone()) {
            Ok(contents) => contents,
            Err(err) => {
                eprintln!("Error reading {path}: {err}");
                std::process::exit(1);
            }
        };

        let mut plan: ActionPlan = match serde_json::from_str(&input) {
            Ok(plan) => plan,
            Err(_) => match build_plan_from_script(
                &input,
                &path,
                cli.intent_model.clone(),
                cli.intent_threshold,
                debug,
            ) {
                Ok((script_plan, script_cfg, script_intent_mode, script_runtime)) => {
                    cfg_override = Some(script_cfg);
                    runtime_override = Some(script_runtime);
                    intent_mode = script_intent_mode;
                    script_setup_path = Some(path.clone());
                    script_plan
                }
                Err(err) => {
                    eprintln!("Error: {err:#}");
                    std::process::exit(1);
                }
            },
        };
        if plan.source.is_none() {
            plan.source = Some(path.to_string());
        }
        plan
    };

    if let (Some(path), Some(cfg)) = (script_setup_path.as_deref(), cfg_override.as_ref()) {
        print_script_setup(path, cfg, runtime_override.as_ref(), cli.flow, debug);
    }

    let code = execute_plan(
        plan,
        cli.flow,
        cli.auto_yes,
        intent_mode,
        cfg_override.as_ref(),
        runtime_override.as_ref(),
        debug,
    );
    if code != 0 {
        std::process::exit(code);
    }
}
