use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;

fn intent_model_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx")
}

fn spawn_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 2048];
            let n = match stream.read(&mut buf) {
                Ok(n) => n,
                Err(_) => continue,
            };
            if n == 0 {
                continue;
            }
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");

            let (status, body) = if path.starts_with("/fee_stats") {
                ("200 OK", r#"{"last_ledger_base_fee":"100"}"#.to_string())
            } else if path.starts_with("/accounts/") {
                (
                    "200 OK",
                    r#"{"balances":[{"asset_type":"native","balance":"10000.0000000"}]}"#
                        .to_string(),
                )
            } else if path.starts_with("/transactions/") {
                ("400 Bad Request", r#"{"status":400}"#.to_string())
            } else if path.starts_with("/friendbot") {
                ("400 Bad Request", "bad request".to_string())
            } else {
                ("404 Not Found", "not found".to_string())
            };

            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{}", addr)
}

#[test]
fn flow_preview_reports_friendbot_and_tx_status_errors() {
    let base_url = spawn_test_server();
    let friendbot_url = format!("{}/friendbot", base_url);

    let tmp = std::env::temp_dir().join("nc_flow_test.nc");
    let input = r#"
stellar.account.balance account="GTEST" asset="XLM"
stellar.account.fund_testnet account="GTEST"
stellar.tx.status hash="ABC123"
"#;
    fs::write(&tmp, input).expect("write temp nc");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_str().unwrap())
        .arg("--flow")
        .arg("--yes")
        .env("NC_STELLAR_HORIZON_URL", &base_url)
        .env("NC_FRIENDBOT_URL", &friendbot_url)
        .output()
        .expect("run neurochain-stellar");

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(combined.contains("Estimated fee: 100 stroops x 0 ops = 0 stroops"));
    assert!(combined.contains("balance GTEST: XLM = 10000.0000000"));
    assert!(combined.contains("friendbot failed"));
    assert!(combined.contains("simulate_error: tx status failed"));
}

#[test]
fn intent_mode_low_confidence_blocks_flow_and_returns_exit_5() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg("Tell me a joke about stars")
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.99")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar in intent mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Intent safety guard blocked flow"));
    assert!(combined.contains("\"kind\": \"unknown\""));
    assert!(!combined.contains("=== Preview ==="));
}

#[test]
fn intent_mode_slot_missing_blocks_flow_and_returns_exit_5() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg("Send XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P")
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar in intent mode with slot missing");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Intent safety guard blocked flow"));
    assert!(combined.contains("slot_missing"));
    assert!(!combined.contains("=== Preview ==="));
}

#[test]
fn intent_mode_allowlist_enforced_blocks_with_exit_3() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg("Send 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P")
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .env("NC_ASSET_ALLOWLIST", "USDC:GISSUER")
        .env("NC_ALLOWLIST_ENFORCE", "1")
        .output()
        .expect("run neurochain-stellar with allowlist enforce");

    assert_eq!(output.status.code(), Some(3));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Allowlist violations (enforced)"));
    assert!(combined.contains("asset XLM"));
}

#[test]
fn intent_mode_policy_enforced_blocks_with_exit_4() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let policy_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join("CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ")
        .join("policy.json");
    if !policy_path.exists() {
        eprintln!("skipping test; missing policy: {}", policy_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg(
            "Call contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello",
        )
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .env(
            "NC_CONTRACT_POLICY",
            policy_path.to_string_lossy().to_string(),
        )
        .env("NC_CONTRACT_POLICY_ENFORCE", "1")
        .output()
        .expect("run neurochain-stellar with contract policy enforce");

    assert_eq!(output.status.code(), Some(4));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Contract policy violations (enforced)"));
    assert!(combined.contains("policy_args_missing"));
}

#[test]
fn intent_mode_policy_typed_slot_error_blocks_with_exit_5() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let policy_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join("CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ")
        .join("policy.json");
    if !policy_path.exists() {
        eprintln!("skipping test; missing policy: {}", policy_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg(
            "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"Hello World\"}",
        )
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .env(
            "NC_CONTRACT_POLICY",
            policy_path.to_string_lossy().to_string(),
        )
        .output()
        .expect("run neurochain-stellar with policy-typed slot mismatch");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_type_error"));
    assert!(combined.contains("Intent safety guard blocked flow"));
    assert!(!combined.contains("=== Preview ==="));
}

#[test]
fn intent_mode_debug_flag_emits_trace_lines() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg("Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P")
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .arg("--debug")
        .output()
        .expect("run neurochain-stellar with --debug");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("[intent-debug]"));
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
}

#[test]
fn intent_mode_policy_typed_v2_normalizes_address_bytes_symbol_u64() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let contract = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let tmp_policy = std::env::temp_dir().join("nc_policy_typed_v2_normalize.json");
    let policy = format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["hello"],
  "args_schema": {{
    "hello": {{
      "required": {{
        "to": "address",
        "blob": "bytes",
        "ticker": "symbol",
        "amount": "u64"
      }},
      "optional": {{}}
    }}
  }}
}}"#
    );
    fs::write(&tmp_policy, policy).expect("write temp policy");

    let prompt = format!(
        "Invoke contract {contract} function hello args={{\"to\":\" {} \",\"blob\":\"0XDE AD_be-EF\",\"ticker\":\" USDC \",\"amount\":\"1_000,000\"}}",
        account.to_ascii_lowercase()
    );
    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg(prompt)
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .env(
            "NC_CONTRACT_POLICY",
            tmp_policy.to_string_lossy().to_string(),
        )
        .output()
        .expect("run neurochain-stellar with typed-v2 normalization");

    let _ = fs::remove_file(&tmp_policy);

    assert!(output.status.success());
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("\"kind\": \"soroban_contract_invoke\""));
    assert!(combined.contains(&format!("\"to\": \"{account}\"")));
    assert!(combined.contains("\"blob\": \"0xdeadbeef\""));
    assert!(combined.contains("\"ticker\": \"USDC\""));
    assert!(combined.contains("\"amount\": 1000000"));
}

#[test]
fn intent_mode_policy_typed_v2_reports_multiple_arg_errors() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let contract = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let tmp_policy = std::env::temp_dir().join("nc_policy_typed_v2_multi_error.json");
    let policy = format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["hello"],
  "args_schema": {{
    "hello": {{
      "required": {{
        "to": "address",
        "blob": "bytes",
        "amount": "u64"
      }},
      "optional": {{
        "ticker": "symbol"
      }}
    }}
  }}
}}"#
    );
    fs::write(&tmp_policy, policy).expect("write temp policy");

    let prompt = format!(
        "Invoke contract {contract} function hello args={{\"to\":\"World\",\"blob\":\"0xABC\",\"ticker\":\" BAD VALUE \",\"amount\":\"18446744073709551616\"}}"
    );
    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg(prompt)
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .env(
            "NC_CONTRACT_POLICY",
            tmp_policy.to_string_lossy().to_string(),
        )
        .output()
        .expect("run neurochain-stellar with typed-v2 multi error");

    let _ = fs::remove_file(&tmp_policy);

    assert_eq!(output.status.code(), Some(5));
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_type_error"));
    assert!(combined.contains("ContractInvoke to"));
    assert!(combined.contains("ContractInvoke blob"));
    assert!(combined.contains("ContractInvoke ticker"));
    assert!(combined.contains("ContractInvoke amount"));
    assert!(combined.contains("Intent safety guard blocked flow"));
}

#[test]
fn intent_mode_flow_submit_happy_path_balance_query() {
    let model_path = intent_model_path();
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let base_url = spawn_test_server();
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg("--intent-text")
        .arg(format!("Check balance for {account} asset XLM"))
        .arg("--intent-model")
        .arg(model_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .env("NC_STELLAR_HORIZON_URL", &base_url)
        .output()
        .expect("run neurochain-stellar flow submit for balance query");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("\"kind\": \"stellar_account_balance\""));
    assert!(combined.contains("=== Preview ==="));
    assert!(combined.contains("Submit results:"));
    assert!(combined.contains("balance"));
}
