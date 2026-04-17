use neurochain::actions::{
    parse_action_plan_from_nc, parse_action_plan_from_txrep, validate_plan, Action, ActionPlan,
    Allowlist,
};
use serde_json::Value;
use std::sync::{Mutex, OnceLock};

static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn with_env_lock<F: FnOnce()>(f: F) {
    let lock = ENV_MUTEX.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();
    f();
}

#[test]
fn parse_txrep_into_action_plan() {
    let input = r#"
{
  "tx": {
    "tx": {
      "operations": [
        {
          "body": {
            "create_account": {
              "destination": "GDEST",
              "starting_balance": "20000000"
            }
          }
        },
        {
          "body": {
            "change_trust": {
              "line": {
                "credit_alphanum4": {
                  "asset_code": "USDC",
                  "issuer": "GISSUER"
                }
              },
              "limit": "10000000000"
            }
          }
        },
        {
          "body": {
            "payment": {
              "destination": "GTO",
              "asset": "native",
              "amount": "50000000"
            }
          }
        }
      ]
    }
  }
}
"#;

    let plan = parse_action_plan_from_txrep(input).expect("parse txrep");
    assert_eq!(plan.actions.len(), 3);

    match &plan.actions[0] {
        Action::StellarAccountCreate {
            destination,
            starting_balance,
        } => {
            assert_eq!(destination, "GDEST");
            assert_eq!(starting_balance, "2");
        }
        _ => panic!("expected create_account"),
    }

    match &plan.actions[1] {
        Action::StellarChangeTrust {
            asset_code,
            asset_issuer,
            limit,
        } => {
            assert_eq!(asset_code, "USDC");
            assert_eq!(asset_issuer, "GISSUER");
            assert_eq!(limit.as_deref(), Some("1000"));
        }
        _ => panic!("expected change_trust"),
    }

    match &plan.actions[2] {
        Action::StellarPayment {
            to,
            amount,
            asset_code,
            asset_issuer,
        } => {
            assert_eq!(to, "GTO");
            assert_eq!(amount, "5");
            assert_eq!(asset_code, "XLM");
            assert!(asset_issuer.is_none());
        }
        _ => panic!("expected payment"),
    }
}

#[test]
fn parse_txrep_soroban_invoke() {
    let input = r#"
{
  "tx": {
    "tx": {
      "operations": [
        {
          "body": {
            "invoke_host_function": {
              "host_function": {
                "invoke_contract": {
                  "contract_address": "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ",
                  "function_name": "hello",
                  "args": [
                    { "str": "World" }
                  ]
                }
              },
              "auth": []
            }
          }
        }
      ]
    }
  }
}
"#;

    let plan = parse_action_plan_from_txrep(input).expect("parse txrep");
    assert_eq!(plan.actions.len(), 1);

    match &plan.actions[0] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(
                contract_id,
                "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ"
            );
            assert_eq!(function, "hello");
            assert_eq!(args[0], Value::String("World".to_string()));
        }
        _ => panic!("expected soroban.contract.invoke"),
    }
}

#[test]
fn parse_txrep_soroban_named_args() {
    let input = r#"
{
  "tx": {
    "tx": {
      "operations": [
        {
          "body": {
            "invoke_host_function": {
              "host_function": {
                "invoke_contract": {
                  "contract_address": "CABC123",
                  "function_name": "transfer",
                  "args": [
                    { "sym": "to" },
                    { "address": { "account": "GDEST" } },
                    { "sym": "amount" },
                    { "i64": "42" }
                  ]
                }
              },
              "auth": []
            }
          }
        }
      ]
    }
  }
}
"#;

    let plan = parse_action_plan_from_txrep(input).expect("parse txrep");
    assert_eq!(plan.actions.len(), 1);

    match &plan.actions[0] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(contract_id, "CABC123");
            assert_eq!(function, "transfer");
            assert_eq!(args["to"], Value::String("GDEST".to_string()));
            assert_eq!(args["amount"], Value::Number(42.into()));
        }
        _ => panic!("expected soroban.contract.invoke"),
    }
}

#[test]
fn parse_manual_nc_actions() {
    let input = r#"
# stellar.account.balance account="G..." asset="XLM"
stellar.account.create destination="G..." starting_balance="2"
stellar.account.fund_testnet account="G..."
stellar.change_trust asset_code="USDC" asset_issuer="G..." limit="1000"
stellar.payment to="G..." amount="5" asset_code="XLM"
stellar.payment to="G..." amount="12.5" asset_code="USDC" asset_issuer="G..."
stellar.tx.status hash="ABC123"
soroban.contract.invoke contract_id="C..." function="transfer" args={"to":"G...","amount":100}
"#;

    let plan = parse_action_plan_from_nc(input);
    assert_eq!(plan.actions.len(), 7);

    matches!(plan.actions[0], Action::StellarAccountCreate { .. });
    matches!(plan.actions[1], Action::StellarAccountFundTestnet { .. });
    matches!(plan.actions[2], Action::StellarChangeTrust { .. });
    matches!(plan.actions[3], Action::StellarPayment { .. });
    matches!(plan.actions[4], Action::StellarPayment { .. });
    matches!(plan.actions[5], Action::StellarTxStatus { .. });

    match &plan.actions[6] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(contract_id, "C...");
            assert_eq!(function, "transfer");
            assert_eq!(args["to"], Value::String("G...".to_string()));
            assert_eq!(args["amount"], Value::Number(100.into()));
        }
        _ => panic!("expected soroban.contract.invoke"),
    }
}

#[test]
fn parse_inline_comments_and_quoted_values() {
    let input = r#"
stellar.account.balance account="G//X" asset="XLM" // trailing comment
# stellar.account.create destination="G DEST" starting_balance="2" # inline comment
soroban.contract.invoke contract_id="C..." function="hello world" args={"to":"G...","note":"hi // there"}
"#;

    let plan = parse_action_plan_from_nc(input);
    assert_eq!(plan.actions.len(), 2);

    match &plan.actions[0] {
        Action::StellarAccountBalance { account, asset } => {
            assert_eq!(account, "G//X");
            assert_eq!(asset.as_deref(), Some("XLM"));
        }
        _ => panic!("expected stellar.account.balance"),
    }

    match &plan.actions[1] {
        Action::SorobanContractInvoke { function, args, .. } => {
            assert_eq!(function, "hello world");
            assert_eq!(args["note"], Value::String("hi // there".to_string()));
        }
        _ => panic!("expected soroban.contract.invoke"),
    }
}

#[test]
fn allowlist_validation_reports_only_invalid() {
    with_env_lock(|| {
        let prev_assets = std::env::var("NC_ASSET_ALLOWLIST").ok();
        let prev_contracts = std::env::var("NC_SOROBAN_ALLOWLIST").ok();

        std::env::set_var("NC_ASSET_ALLOWLIST", "XLM,USDC:GISSUER");
        std::env::set_var("NC_SOROBAN_ALLOWLIST", "C1:transfer");

        let allowlist = Allowlist::from_env();
        let plan = ActionPlan {
            schema_version: 1,
            actions: vec![
                Action::StellarPayment {
                    to: "GDEST".to_string(),
                    amount: "1".to_string(),
                    asset_code: "XLM".to_string(),
                    asset_issuer: None,
                },
                Action::StellarPayment {
                    to: "GDEST".to_string(),
                    amount: "2".to_string(),
                    asset_code: "USDC".to_string(),
                    asset_issuer: Some("GISSUER".to_string()),
                },
                Action::StellarPayment {
                    to: "GDEST".to_string(),
                    amount: "3".to_string(),
                    asset_code: "BAD".to_string(),
                    asset_issuer: None,
                },
                Action::SorobanContractInvoke {
                    contract_id: "C1".to_string(),
                    function: "transfer".to_string(),
                    args: Value::Null,
                },
                Action::SorobanContractInvoke {
                    contract_id: "C2".to_string(),
                    function: "mint".to_string(),
                    args: Value::Null,
                },
            ],
            warnings: vec![],
            source: None,
        };

        let violations = validate_plan(&plan, &allowlist);
        assert_eq!(violations.len(), 2);
        assert!(violations.iter().any(|v| v.reason.contains("asset BAD")));
        assert!(violations
            .iter()
            .any(|v| v.reason.contains("contract C2:mint")));

        if let Some(v) = prev_assets {
            std::env::set_var("NC_ASSET_ALLOWLIST", v);
        } else {
            std::env::remove_var("NC_ASSET_ALLOWLIST");
        }
        if let Some(v) = prev_contracts {
            std::env::set_var("NC_SOROBAN_ALLOWLIST", v);
        } else {
            std::env::remove_var("NC_SOROBAN_ALLOWLIST");
        }
    });
}
