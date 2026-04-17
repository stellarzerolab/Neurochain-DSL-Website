use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPlan {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Default for ActionPlan {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            actions: Vec::new(),
            warnings: Vec::new(),
            source: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    StellarAccountBalance {
        account: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asset: Option<String>,
    },
    StellarAccountCreate {
        destination: String,
        starting_balance: String,
    },
    StellarAccountFundTestnet {
        account: String,
    },
    StellarChangeTrust {
        asset_code: String,
        asset_issuer: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<String>,
    },
    StellarPayment {
        to: String,
        amount: String,
        asset_code: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asset_issuer: Option<String>,
    },
    StellarTxStatus {
        hash: String,
    },
    SorobanContractDeploy {
        alias: String,
        wasm: String,
    },
    SorobanContractInvoke {
        contract_id: String,
        function: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    Unknown {
        reason: String,
    },
}

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::StellarAccountBalance { .. } => "stellar.account.balance",
            Action::StellarAccountCreate { .. } => "stellar.account.create",
            Action::StellarAccountFundTestnet { .. } => "stellar.account.fund_testnet",
            Action::StellarChangeTrust { .. } => "stellar.change_trust",
            Action::StellarPayment { .. } => "stellar.payment",
            Action::StellarTxStatus { .. } => "stellar.tx.status",
            Action::SorobanContractDeploy { .. } => "soroban.contract.deploy",
            Action::SorobanContractInvoke { .. } => "soroban.contract.invoke",
            Action::Unknown { .. } => "unknown",
        }
    }
}

#[derive(Debug, Default)]
pub struct Allowlist {
    assets: HashSet<String>,
    contracts: HashSet<String>,
}

impl Allowlist {
    pub fn from_env() -> Self {
        let assets = parse_allowlist(std::env::var("NC_ASSET_ALLOWLIST").unwrap_or_default());
        let contracts = parse_allowlist(std::env::var("NC_SOROBAN_ALLOWLIST").unwrap_or_default());
        Self { assets, contracts }
    }

    pub fn from_raw(asset_allowlist: &str, contract_allowlist: &str) -> Self {
        let assets = parse_allowlist(asset_allowlist.to_string());
        let contracts = parse_allowlist(contract_allowlist.to_string());
        Self { assets, contracts }
    }

    fn is_asset_allowed(&self, code: &str, issuer: Option<&str>) -> bool {
        if self.assets.is_empty() {
            return true;
        }
        if code.eq_ignore_ascii_case("XLM") {
            return self.assets.contains("XLM");
        }
        let issuer = issuer.unwrap_or("");
        let full = format!("{code}:{issuer}");
        self.assets.contains(&full) || self.assets.contains(code)
    }

    fn is_contract_allowed(&self, contract_id: &str, function: &str) -> bool {
        if self.contracts.is_empty() {
            return true;
        }
        let full = format!("{contract_id}:{function}");
        self.contracts.contains(&full) || self.contracts.contains(contract_id)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowlistViolation {
    pub index: usize,
    pub action: String,
    pub reason: String,
}

pub fn validate_plan(plan: &ActionPlan, allowlist: &Allowlist) -> Vec<AllowlistViolation> {
    let mut violations = Vec::new();

    for (idx, action) in plan.actions.iter().enumerate() {
        match action {
            Action::SorobanContractInvoke {
                contract_id,
                function,
                ..
            } => {
                if !allowlist.is_contract_allowed(contract_id, function) {
                    violations.push(AllowlistViolation {
                        index: idx,
                        action: action.kind().to_string(),
                        reason: format!("contract {contract_id}:{function} not in allowlist"),
                    });
                }
            }
            Action::StellarPayment {
                asset_code,
                asset_issuer,
                ..
            } => {
                if !allowlist.is_asset_allowed(asset_code, asset_issuer.as_deref()) {
                    violations.push(AllowlistViolation {
                        index: idx,
                        action: action.kind().to_string(),
                        reason: format!(
                            "asset {asset_code}:{} not in allowlist",
                            asset_issuer.as_deref().unwrap_or("")
                        ),
                    });
                }
            }
            Action::StellarChangeTrust {
                asset_code,
                asset_issuer,
                ..
            } => {
                if !allowlist.is_asset_allowed(asset_code, Some(asset_issuer)) {
                    violations.push(AllowlistViolation {
                        index: idx,
                        action: action.kind().to_string(),
                        reason: format!("asset {asset_code}:{asset_issuer} not in allowlist"),
                    });
                }
            }
            _ => {}
        }
    }

    violations
}

pub fn enforce_allowlist(
    plan: &ActionPlan,
    allowlist: &Allowlist,
) -> Result<(), Vec<AllowlistViolation>> {
    let violations = validate_plan(plan, allowlist);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn parse_allowlist(raw: String) -> HashSet<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

fn strip_inline_comment(line: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(active) = quote {
            if ch == '\\' {
                out.push(ch);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
                continue;
            }
            if ch == active {
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
        if ch == '/' && chars.peek() == Some(&'/') {
            break;
        }

        out.push(ch);
    }

    out.trim_end().to_string()
}

pub fn parse_action_plan_from_nc(contents: &str) -> ActionPlan {
    let mut plan = ActionPlan::default();

    for (idx, raw_line) in contents.lines().enumerate() {
        let mut line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if let Some(stripped) = line.strip_prefix("action ") {
            line = stripped.trim_start();
        }

        let line = strip_inline_comment(line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if !(line.starts_with("stellar.") || line.starts_with("soroban.")) {
            continue;
        }

        let (line_no_args, args_raw) = split_args_tail(line);
        let tokens = split_tokens(line_no_args);
        if tokens.is_empty() {
            continue;
        }
        let kind = tokens[0].as_str();
        let kv = parse_key_values(&tokens[1..]);

        let action = match kind {
            "stellar.account.balance" => {
                if let Some(account) = kv.get("account") {
                    Action::StellarAccountBalance {
                        account: account.clone(),
                        asset: kv.get("asset").cloned(),
                    }
                } else {
                    Action::Unknown {
                        reason: format!("line {}: missing account", idx + 1),
                    }
                }
            }
            "stellar.account.create" => {
                let destination = kv.get("destination");
                let starting_balance = kv.get("starting_balance");
                match (destination, starting_balance) {
                    (Some(destination), Some(starting_balance)) => Action::StellarAccountCreate {
                        destination: destination.clone(),
                        starting_balance: starting_balance.clone(),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing destination/starting_balance", idx + 1),
                    },
                }
            }
            "stellar.account.fund_testnet" => {
                if let Some(account) = kv.get("account") {
                    Action::StellarAccountFundTestnet {
                        account: account.clone(),
                    }
                } else {
                    Action::Unknown {
                        reason: format!("line {}: missing account", idx + 1),
                    }
                }
            }
            "stellar.change_trust" => {
                let asset_code = kv.get("asset_code");
                let asset_issuer = kv.get("asset_issuer");
                match (asset_code, asset_issuer) {
                    (Some(asset_code), Some(asset_issuer)) => Action::StellarChangeTrust {
                        asset_code: asset_code.clone(),
                        asset_issuer: asset_issuer.clone(),
                        limit: kv.get("limit").cloned(),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing asset_code/asset_issuer", idx + 1),
                    },
                }
            }
            "stellar.payment" => {
                let to = kv.get("to");
                let amount = kv.get("amount");
                let asset_code = kv.get("asset_code");
                match (to, amount, asset_code) {
                    (Some(to), Some(amount), Some(asset_code)) => Action::StellarPayment {
                        to: to.clone(),
                        amount: amount.clone(),
                        asset_code: asset_code.clone(),
                        asset_issuer: kv.get("asset_issuer").cloned(),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing to/amount/asset_code", idx + 1),
                    },
                }
            }
            "stellar.tx.status" => {
                if let Some(hash) = kv.get("hash") {
                    Action::StellarTxStatus { hash: hash.clone() }
                } else {
                    Action::Unknown {
                        reason: format!("line {}: missing hash", idx + 1),
                    }
                }
            }
            "soroban.contract.deploy" => {
                let alias = kv.get("alias");
                let wasm = kv.get("wasm");
                match (alias, wasm) {
                    (Some(alias), Some(wasm)) => Action::SorobanContractDeploy {
                        alias: alias.clone(),
                        wasm: wasm.clone(),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing alias/wasm", idx + 1),
                    },
                }
            }
            "soroban.contract.invoke" => {
                let contract_id = kv.get("contract_id");
                let function = kv.get("function");
                match (contract_id, function) {
                    (Some(contract_id), Some(function)) => Action::SorobanContractInvoke {
                        contract_id: contract_id.clone(),
                        function: function.clone(),
                        args: parse_args_json(args_raw).unwrap_or(serde_json::Value::Null),
                    },
                    _ => Action::Unknown {
                        reason: format!("line {}: missing contract_id/function", idx + 1),
                    },
                }
            }
            _ => Action::Unknown {
                reason: format!("line {}: unknown action '{kind}'", idx + 1),
            },
        };

        if let Action::Unknown { reason } = &action {
            plan.warnings.push(reason.to_string());
        }
        plan.actions.push(action);
    }

    if plan.actions.is_empty() {
        plan.warnings
            .push("no actions detected in .nc input".to_string());
    }

    plan
}

pub fn parse_action_plan_from_txrep(input: &str) -> Result<ActionPlan, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|err| format!("invalid json: {err}"))?;
    let ops = extract_tx_operations(&value).ok_or_else(|| "missing operations".to_string())?;

    let mut plan = ActionPlan {
        source: Some("txrep".to_string()),
        ..ActionPlan::default()
    };

    for (idx, op) in ops.iter().enumerate() {
        let body = op.get("body").unwrap_or(op);

        if let Some(create_account) = body.get("create_account") {
            let destination = get_string(create_account, "destination")
                .ok_or_else(|| format!("create_account missing destination at op {idx}"))?;
            let starting_balance = amount_from_value(create_account.get("starting_balance"))
                .ok_or_else(|| format!("create_account missing starting_balance at op {idx}"))?;
            plan.actions.push(Action::StellarAccountCreate {
                destination,
                starting_balance,
            });
            continue;
        }

        if let Some(change_trust) = body.get("change_trust") {
            let line = change_trust
                .get("line")
                .ok_or_else(|| format!("change_trust missing line at op {idx}"))?;
            let (asset_code, asset_issuer) = parse_asset(line)
                .ok_or_else(|| format!("change_trust invalid asset at op {idx}"))?;
            let asset_issuer =
                asset_issuer.ok_or_else(|| format!("change_trust missing issuer at op {idx}"))?;
            let limit = amount_from_value(change_trust.get("limit"))
                .or(Some("0".to_string()))
                .filter(|v| v != "0");
            plan.actions.push(Action::StellarChangeTrust {
                asset_code,
                asset_issuer,
                limit,
            });
            continue;
        }

        if let Some(payment) = body.get("payment") {
            let destination = get_string(payment, "destination")
                .ok_or_else(|| format!("payment missing destination at op {idx}"))?;
            let amount = amount_from_value(payment.get("amount"))
                .ok_or_else(|| format!("payment missing amount at op {idx}"))?;
            let asset = payment
                .get("asset")
                .ok_or_else(|| format!("payment missing asset at op {idx}"))?;
            let (asset_code, asset_issuer) =
                parse_asset(asset).ok_or_else(|| format!("payment invalid asset at op {idx}"))?;
            plan.actions.push(Action::StellarPayment {
                to: destination,
                amount,
                asset_code,
                asset_issuer,
            });
            continue;
        }

        if let Some(action) = parse_soroban_invoke(body, idx) {
            plan.actions.push(action);
            continue;
        }

        plan.warnings
            .push(format!("unsupported operation at index {idx}"));
        plan.actions.push(Action::Unknown {
            reason: format!("unsupported operation at index {idx}"),
        });
    }

    if plan.actions.is_empty() {
        plan.warnings
            .push("no actions detected in txrep input".to_string());
    }

    Ok(plan)
}

fn extract_tx_operations(value: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    if let Some(ops) = value.get("operations").and_then(|v| v.as_array()) {
        return Some(ops);
    }
    if let Some(tx) = value.get("tx") {
        if let Some(ops) = tx.get("operations").and_then(|v| v.as_array()) {
            return Some(ops);
        }
        if let Some(inner) = tx.get("tx") {
            if let Some(ops) = inner.get("operations").and_then(|v| v.as_array()) {
                return Some(ops);
            }
        }
    }
    None
}

fn get_string(value: &serde_json::Value, field: &str) -> Option<String> {
    value.get(field).and_then(|v| match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn amount_from_value(value: Option<&serde_json::Value>) -> Option<String> {
    let raw = match value? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };

    if raw.contains('.') {
        return Some(raw);
    }
    let parsed: i128 = raw.parse().ok()?;
    Some(format_stroops(parsed))
}

fn format_stroops(amount: i128) -> String {
    let negative = amount < 0;
    let value = amount.abs();
    let whole = value / 10_000_000;
    let frac = value % 10_000_000;
    let mut out = format!("{whole}.{frac:07}");
    while out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
    if out.is_empty() {
        out = "0".to_string();
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}

fn parse_asset(value: &serde_json::Value) -> Option<(String, Option<String>)> {
    match value {
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("native") => {
            return Some(("XLM".to_string(), None));
        }
        serde_json::Value::Object(obj) => {
            if let Some(inner) = obj
                .get("credit_alphanum4")
                .or_else(|| obj.get("credit_alphanum12"))
            {
                let asset_code = inner.get("asset_code")?.as_str()?.to_string();
                let issuer = inner.get("issuer")?.as_str()?.to_string();
                return Some((asset_code, Some(issuer)));
            }
        }
        _ => {}
    }
    None
}

fn parse_soroban_invoke(body: &serde_json::Value, idx: usize) -> Option<Action> {
    let invoke = body
        .get("invoke_host_function")
        .or_else(|| body.get("invoke_host_function_op"))?;
    let host_function = invoke
        .get("host_function")
        .or_else(|| invoke.get("host_function_op"))
        .unwrap_or(invoke);
    let invoke_contract = host_function.get("invoke_contract")?;

    let contract_id = parse_contract_id(invoke_contract)?;
    let function = invoke_contract
        .get("function_name")
        .or_else(|| invoke_contract.get("function"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let args = invoke_contract
        .get("args")
        .and_then(|v| v.as_array())
        .map(|items| {
            let decoded: Vec<serde_json::Value> = items.iter().filter_map(scval_to_json).collect();
            normalize_soroban_args(decoded)
        })
        .unwrap_or(serde_json::Value::Null);

    Some(Action::SorobanContractInvoke {
        contract_id,
        function: if function.is_empty() {
            format!("unknown_{idx}")
        } else {
            function
        },
        args,
    })
}

fn parse_contract_id(invoke_contract: &serde_json::Value) -> Option<String> {
    let candidate = invoke_contract
        .get("contract_address")
        .or_else(|| invoke_contract.get("contract_id"))?;

    if let Some(id) = candidate.as_str() {
        return Some(id.to_string());
    }

    if let Some(obj) = candidate.as_object() {
        if let Some(id) = obj
            .get("contract_id")
            .or_else(|| obj.get("id"))
            .and_then(|v| v.as_str())
        {
            return Some(id.to_string());
        }
        if let Some(inner) = obj.get("contract") {
            if let Some(id) = inner.as_str() {
                return Some(id.to_string());
            }
            if let Some(id) = inner.get("contract_id").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
    }
    None
}

fn scval_to_json(value: &serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {
            return Some(value.clone());
        }
        serde_json::Value::Array(items) => {
            let decoded: Vec<serde_json::Value> = items.iter().filter_map(scval_to_json).collect();
            return Some(serde_json::Value::Array(decoded));
        }
        serde_json::Value::Object(map) => {
            if let Some(v) = map.get("sym").and_then(|v| v.as_str()) {
                return Some(serde_json::Value::String(v.to_string()));
            }
            if let Some(v) = map.get("str").and_then(|v| v.as_str()) {
                return Some(serde_json::Value::String(v.to_string()));
            }
            if let Some(v) = map.get("bool").and_then(|v| v.as_bool()) {
                return Some(serde_json::Value::Bool(v));
            }
            let numeric = map
                .get("i64")
                .or_else(|| map.get("i32"))
                .or_else(|| map.get("u32"))
                .or_else(|| map.get("u64"));
            if let Some(v) = numeric {
                if let Some(num) = v.as_i64() {
                    return Some(serde_json::Value::Number(num.into()));
                }
                if let Some(num) = v.as_u64() {
                    return Some(serde_json::Value::Number(num.into()));
                }
                if let Some(raw) = v.as_str() {
                    if let Ok(num) = raw.parse::<i64>() {
                        return Some(serde_json::Value::Number(num.into()));
                    }
                    return Some(serde_json::Value::String(raw.to_string()));
                }
            }
            if let Some(v) = map.get("i128").or_else(|| map.get("u128")) {
                if let Some(raw) = v.as_str() {
                    return Some(serde_json::Value::String(raw.to_string()));
                }
            }
            if let Some(v) = map.get("bytes").and_then(|v| v.as_str()) {
                return Some(serde_json::Value::String(v.to_string()));
            }
            if let Some(address) = map.get("address") {
                if let Some(account) = address
                    .get("account")
                    .or_else(|| address.get("account_id"))
                    .and_then(|v| v.as_str())
                {
                    return Some(serde_json::Value::String(account.to_string()));
                }
                if let Some(contract) = address.get("contract").and_then(|v| v.as_str()) {
                    return Some(serde_json::Value::String(contract.to_string()));
                }
                if let Some(contract_id) = address.get("contract_id").and_then(|v| v.as_str()) {
                    return Some(serde_json::Value::String(contract_id.to_string()));
                }
            }
            if let Some(vec_val) = map.get("vec").and_then(|v| v.as_array()) {
                let decoded: Vec<serde_json::Value> =
                    vec_val.iter().filter_map(scval_to_json).collect();
                return Some(serde_json::Value::Array(decoded));
            }
            if let Some(map_val) = map.get("map").and_then(|v| v.as_array()) {
                let mut obj = serde_json::Map::new();
                let mut fallback = Vec::new();
                for entry in map_val {
                    let key = entry.get("key").and_then(scval_to_json);
                    let val = entry.get("val").and_then(scval_to_json);
                    match (key, val) {
                        (Some(serde_json::Value::String(k)), Some(v)) => {
                            obj.insert(k, v);
                        }
                        (Some(k), Some(v)) => {
                            fallback.push(serde_json::json!({ "key": k, "val": v }));
                        }
                        _ => {}
                    }
                }
                if !obj.is_empty() {
                    return Some(serde_json::Value::Object(obj));
                }
                if !fallback.is_empty() {
                    return Some(serde_json::Value::Array(fallback));
                }
            }
        }
        _ => {}
    }
    None
}

fn normalize_soroban_args(args: Vec<serde_json::Value>) -> serde_json::Value {
    if args.len() >= 2 && args.len().is_multiple_of(2) {
        let mut map = serde_json::Map::new();
        for pair in args.chunks(2) {
            let Some(key) = pair[0].as_str() else {
                return serde_json::Value::Array(args);
            };
            if map.contains_key(key) {
                return serde_json::Value::Array(args);
            }
            map.insert(key.to_string(), pair[1].clone());
        }
        if !map.is_empty() {
            return serde_json::Value::Object(map);
        }
    }
    serde_json::Value::Array(args)
}

fn split_args_tail(line: &str) -> (&str, Option<&str>) {
    if let Some(pos) = line.find(" args=") {
        let head = line[..pos].trim();
        let tail = line[pos + 6..].trim();
        return (head, if tail.is_empty() { None } else { Some(tail) });
    }
    (line, None)
}

fn parse_args_json(raw: Option<&str>) -> Option<serde_json::Value> {
    let raw = raw?;
    if raw.starts_with('{') || raw.starts_with('[') {
        return serde_json::from_str(raw).ok();
    }
    if raw.starts_with('"') || raw.starts_with('\'') {
        let unquoted = unquote(raw);
        return serde_json::from_str(&unquoted)
            .ok()
            .or(Some(serde_json::Value::String(unquoted)));
    }
    Some(serde_json::Value::String(raw.to_string()))
}

fn split_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(active) = quote {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
                continue;
            }
            if ch == active {
                quote = None;
                continue;
            }
            current.push(ch);
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
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

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn parse_key_values(tokens: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for token in tokens {
        if let Some((key, value)) = token.split_once('=') {
            let value = unquote(value);
            map.insert(key.to_string(), value);
        }
    }
    map
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}
