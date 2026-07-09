use std::env;
use std::fs;
use std::io::{self, Read};

use neurochain::actions::{parse_action_plan_from_txrep, Action};
use neurochain::banner;
use serde::Serialize;

#[derive(Serialize)]
struct DatasetRow {
    text: String,
    intent: String,
    slots: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<Action>,
}

fn print_usage() {
    eprintln!("Usage: txrep-to-jsonl <txrep.json|->");
    eprintln!("Outputs JSONL rows: {{text,intent,slots,action}}");
}

fn read_input(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|err| format!("failed to read stdin: {err}"))?;
        return Ok(buf);
    }
    fs::read_to_string(path).map_err(|err| format!("failed to read {path}: {err}"))
}

fn main() {
    banner::print_banner_stderr();
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        print_usage();
        std::process::exit(2);
    };

    let input = match read_input(&path) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    };

    let plan = match parse_action_plan_from_txrep(&input) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    };

    for action in plan.actions {
        let (intent, slots, text) = action_to_dataset(&action);
        let row = DatasetRow {
            text,
            intent,
            slots,
            action: Some(action),
        };
        let json = serde_json::to_string(&row).unwrap_or_else(|err| {
            eprintln!("Error: failed to serialize row: {err}");
            std::process::exit(1);
        });
        println!("{json}");
    }
}

fn action_to_dataset(action: &Action) -> (String, serde_json::Value, String) {
    match action {
        Action::StellarAccountCreate {
            destination,
            starting_balance,
        } => {
            let intent = "CreateAccount".to_string();
            let slots = serde_json::json!({
                "destination": destination,
                "starting_balance": starting_balance,
            });
            let text = format!("Create account {destination} with {starting_balance} XLM");
            (intent, slots, text)
        }
        Action::StellarChangeTrust {
            asset_code,
            asset_issuer,
            limit,
        } => {
            let intent = "ChangeTrust".to_string();
            let slots = serde_json::json!({
                "asset_code": asset_code,
                "asset_issuer": asset_issuer,
                "limit": limit,
            });
            let limit_text = limit.as_deref().unwrap_or("max");
            let text = format!("Add trustline {asset_code}:{asset_issuer} limit {limit_text}");
            (intent, slots, text)
        }
        Action::StellarPayment {
            to,
            amount,
            asset_code,
            asset_issuer,
        } => {
            let (intent, asset_label) = if asset_code.eq_ignore_ascii_case("XLM") {
                ("TransferXLM".to_string(), "XLM".to_string())
            } else {
                let issuer = asset_issuer.clone().unwrap_or_default();
                (
                    "TransferAsset".to_string(),
                    format!("{asset_code}:{issuer}"),
                )
            };
            let slots = serde_json::json!({
                "to": to,
                "amount": amount,
                "asset_code": asset_code,
                "asset_issuer": asset_issuer,
            });
            let text = format!("Send {amount} {asset_label} to {to}");
            (intent, slots, text)
        }
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            let intent = "ContractInvoke".to_string();
            let slots = serde_json::json!({
                "contract_id": contract_id,
                "function": function,
                "args": args,
            });
            let text = format!("Invoke contract {contract_id} function {function}");
            (intent, slots, text)
        }
        Action::SorobanContractDeploy { alias, wasm } => {
            let intent = "ContractDeploy".to_string();
            let slots = serde_json::json!({
                "alias": alias,
                "wasm": wasm,
            });
            let text = format!("Deploy contract alias {alias} wasm {wasm}");
            (intent, slots, text)
        }
        Action::StellarAccountBalance { account, asset } => {
            let intent = "BalanceQuery".to_string();
            let slots = serde_json::json!({
                "account": account,
                "asset": asset,
            });
            let asset_label = asset.as_deref().unwrap_or("XLM");
            let text = format!("Check balance {account} asset {asset_label}");
            (intent, slots, text)
        }
        Action::StellarAccountFundTestnet { account } => {
            let intent = "FundTestnet".to_string();
            let slots = serde_json::json!({ "account": account });
            let text = format!("Fund testnet account {account}");
            (intent, slots, text)
        }
        Action::StellarTxStatus { hash } => {
            let intent = "TxStatus".to_string();
            let slots = serde_json::json!({ "hash": hash });
            let text = format!("Check tx status {hash}");
            (intent, slots, text)
        }
        Action::Unknown { reason } => {
            let intent = "Unknown".to_string();
            let slots = serde_json::json!({ "reason": reason });
            let text = format!("Unknown action: {reason}");
            (intent, slots, text)
        }
    }
}
