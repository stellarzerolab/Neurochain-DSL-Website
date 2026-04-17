use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use tempfile::TempDir;

fn create_fake_stellar_cli() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir for fake stellar cli");
    #[cfg(windows)]
    let cli_path = dir.path().join("stellar.cmd");
    #[cfg(not(windows))]
    let cli_path = dir.path().join("stellar");

    #[cfg(windows)]
    let script = "@echo off\r\nif \"%1\"==\"keys\" if \"%2\"==\"generate\" (echo generated alias %3& exit /b 0)\r\nif \"%1\"==\"keys\" if \"%2\"==\"address\" (echo GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX& exit /b 0)\r\necho unexpected args %* 1>&2\r\nexit /b 1\r\n";
    #[cfg(not(windows))]
    let script = "#!/usr/bin/env sh\nif [ \"$1\" = \"keys\" ] && [ \"$2\" = \"generate\" ]; then\n  echo \"generated alias $3\"\n  exit 0\nfi\nif [ \"$1\" = \"keys\" ] && [ \"$2\" = \"address\" ]; then\n  echo \"GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX\"\n  exit 0\nfi\necho \"unexpected args: $@\" >&2\nexit 1\n";

    fs::write(&cli_path, script).expect("write fake stellar cli");
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&cli_path)
            .expect("metadata for fake stellar cli")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&cli_path, perms).expect("chmod fake stellar cli");
    }

    (dir, cli_path)
}

fn spawn_friendbot_server() -> (String, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind friendbot test server");
    let addr = listener
        .local_addr()
        .expect("friendbot test server local addr");
    let seen_friendbot = Arc::new(AtomicBool::new(false));
    let seen_friendbot_bg = Arc::clone(&seen_friendbot);

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

            let (status, body) = if path.starts_with("/friendbot") {
                seen_friendbot_bg.store(true, Ordering::SeqCst);
                ("200 OK", r#"{"result":"ok"}"#)
            } else {
                ("404 Not Found", r#"{"error":"not found"}"#)
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    (format!("http://{}/friendbot", addr), seen_friendbot)
}

fn create_fake_stellar_cli_with_soroban_invoke() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir for fake stellar cli");
    #[cfg(windows)]
    let cli_path = dir.path().join("stellar.cmd");
    #[cfg(not(windows))]
    let cli_path = dir.path().join("stellar");

    #[cfg(windows)]
    let script = "@echo off\r\nif \"%1\"==\"keys\" if \"%2\"==\"address\" (echo GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX& exit /b 0)\r\nif \"%1\"==\"contract\" if \"%2\"==\"invoke\" (echo [\"Hello\",\"World\"]& exit /b 0)\r\necho unexpected args %* 1>&2\r\nexit /b 1\r\n";
    #[cfg(not(windows))]
    let script = "#!/usr/bin/env sh\nif [ \"$1\" = \"keys\" ] && [ \"$2\" = \"address\" ]; then\n  echo \"GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX\"\n  exit 0\nfi\nif [ \"$1\" = \"contract\" ] && [ \"$2\" = \"invoke\" ]; then\n  echo '[\"Hello\",\"World\"]'\n  exit 0\nfi\necho \"unexpected args: $@\" >&2\nexit 1\n";

    fs::write(&cli_path, script).expect("write fake stellar cli");
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&cli_path)
            .expect("metadata for fake stellar cli")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&cli_path, perms).expect("chmod fake stellar cli");
    }

    (dir, cli_path)
}

fn spawn_horizon_tx_server(expected_account: &str, tx_hash: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind horizon test server");
    let addr = listener
        .local_addr()
        .expect("horizon test server local addr");
    let expected_prefix = format!("/accounts/{expected_account}/transactions");
    let body_ok = format!(r#"{{"_embedded":{{"records":[{{"hash":"{tx_hash}"}}]}}}}"#);

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

            let (status, body) = if path.starts_with(&expected_prefix) {
                ("200 OK", body_ok.as_str())
            } else {
                ("404 Not Found", r#"{"error":"not found"}"#)
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    format!("http://{addr}")
}

#[test]
fn nc_script_supports_ai_network_wallet_and_intent_lines() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_intent_mode.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
network: testnet
wallet: nc-testnet
set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Script execution setup:"));
    assert!(combined.contains("- source:"));
    assert!(combined.contains("- network: testnet"));
    assert!(combined.contains("- flow_mode: off"));
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"asset_code\": \"XLM\""));
    assert!(combined.contains("intent_model: path=models/intent_stellar/model.onnx"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_wallet_generate_sets_wallet_source_alias() {
    let (_tmp_dir, fake_cli) = create_fake_stellar_cli();
    let tmp = std::env::temp_dir().join("nc_script_wallet_generate.nc");
    let script = format!(
        "stellar_cli: \"{}\"\nwallet_generate: demo-script\nnetwork: testnet\n",
        fake_cli.to_string_lossy()
    );
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .env_remove("NC_SOROBAN_SOURCE")
        .env_remove("NC_STELLAR_SOURCE")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Script execution setup:"));
    assert!(combined.contains("- wallet/source: demo-script"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_wallet_bootstrap_sets_wallet_source_and_calls_friendbot() {
    let (_tmp_dir, fake_cli) = create_fake_stellar_cli();
    let (friendbot_url, seen_friendbot) = spawn_friendbot_server();
    let tmp = std::env::temp_dir().join("nc_script_wallet_bootstrap.nc");
    let script = format!(
        "stellar_cli: \"{}\"\nfriendbot: \"{}\"\nwallet_bootstrap: demo-script-boot\nnetwork: testnet\n",
        fake_cli.to_string_lossy(),
        friendbot_url
    );
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .env_remove("NC_SOROBAN_SOURCE")
        .env_remove("NC_STELLAR_SOURCE")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Script execution setup:"));
    assert!(combined.contains("- wallet/source: demo-script-boot"));
    assert!(
        seen_friendbot.load(Ordering::SeqCst),
        "expected wallet_bootstrap to call friendbot endpoint"
    );

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_soroban_submit_resolves_alias_for_latest_hash_fallback() {
    let (_tmp_dir, fake_cli) = create_fake_stellar_cli_with_soroban_invoke();
    let tx_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let horizon_url = spawn_horizon_tx_server(account, tx_hash);

    let tmp = std::env::temp_dir().join("nc_script_soroban_submit_latest_hash_from_alias.nc");
    let script = format!(
        "stellar_cli: \"{}\"\nhorizon: \"{}\"\nnetwork: testnet\nwallet: nc-testnet\nsoroban.contract.invoke contract_id=\"CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ\" function=\"hello\" args={{\"to\":\"World\"}}\n",
        fake_cli.to_string_lossy(),
        horizon_url
    );
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Submit results:"));
    assert!(combined.contains(
        "soroban submit CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ:hello | status=ok"
    ));
    assert!(combined.contains(&format!("tx_hash={tx_hash}")));
    assert!(combined.contains("return=[\"Hello\",\"World\"] (latest)"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_debug_setting_emits_intent_trace_lines() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_intent_debug_trace.nc");
    let script = r#"
debug
AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.20
set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("[intent-debug]"));
    assert!(combined.contains("\"kind\": \"stellar_payment\""));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_supports_if_gate_with_multiple_models() {
    let intent_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    let sst2_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("distilbert-sst2")
        .join("model.onnx");
    if !intent_model.exists() {
        eprintln!("skipping test; missing model: {}", intent_model.display());
        return;
    }
    if !sst2_model.exists() {
        eprintln!("skipping test; missing model: {}", sst2_model.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_if_gate_multimodel.nc");
    let script = r#"
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "This is wonderful!"
if mood == "Positive":
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
else:
    neuro "No payment"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"asset_code\": \"XLM\""));
    assert!(combined.contains("intent_model: path=models/intent_stellar/model.onnx"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_model_agnostic_gate_golden_path_produces_payment_plan() {
    let intent_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    let sst2_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("distilbert-sst2")
        .join("model.onnx");
    if !intent_model.exists() {
        eprintln!("skipping test; missing model: {}", intent_model.display());
        return;
    }
    if !sst2_model.exists() {
        eprintln!("skipping test; missing model: {}", sst2_model.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_model_agnostic_gate_golden.nc");
    let script = r#"
network: testnet
wallet: nc-testnet
AI: "models/distilbert-sst2/model.onnx"
set gate from AI: "This is wonderful!"
set allow_label = "Positive"
if gate == allow_label:
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
else:
    neuro "Gate blocked payment"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"asset_code\": \"XLM\""));
    assert!(combined.contains("Script execution setup:"));
    assert!(combined.contains("- network: testnet"));
    assert!(combined.contains("- source:"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn example_golden_path_model_agnostic_produces_payment_plan() {
    let intent_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    let sst2_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("distilbert-sst2")
        .join("model.onnx");
    if !intent_model.exists() {
        eprintln!("skipping test; missing model: {}", intent_model.display());
        return;
    }
    if !sst2_model.exists() {
        eprintln!("skipping test; missing model: {}", sst2_model.display());
        return;
    }

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("golden_path_model_agnostic.nc");
    if !script_path.exists() {
        eprintln!("skipping test; missing example: {}", script_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(script_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-stellar golden-path example");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"asset_code\": \"XLM\""));
    assert!(combined.contains("golden_path_model_agnostic.nc"));
}

#[test]
fn example_golden_path_model_agnostic_blocked_skips_payment() {
    let intent_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    let sst2_model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("distilbert-sst2")
        .join("model.onnx");
    if !intent_model.exists() {
        eprintln!("skipping test; missing model: {}", intent_model.display());
        return;
    }
    if !sst2_model.exists() {
        eprintln!("skipping test; missing model: {}", sst2_model.display());
        return;
    }

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("golden_path_model_agnostic_blocked.nc");
    if !script_path.exists() {
        eprintln!("skipping test; missing example: {}", script_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(script_path.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.20")
        .output()
        .expect("run neurochain-stellar blocked golden-path example");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"actions\": []"));
    assert!(combined.contains("golden_path_model_agnostic_blocked.nc"));
}

#[test]
fn example_policy_typed_stage2_normalize_showcases_multiple_normalizations() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let policy_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("intent_stellar_policy_typed_stage2_demo_policy.json");
    if !policy_path.exists() {
        eprintln!("skipping test; missing policy: {}", policy_path.display());
        return;
    }

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("intent_stellar_policy_typed_stage2_normalize.nc");
    if !script_path.exists() {
        eprintln!("skipping test; missing example: {}", script_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(script_path.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar stage2 normalization example");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined
            .matches("\"kind\": \"soroban_contract_invoke\"")
            .count()
            >= 3
    );
    assert!(combined.contains("\"function\": \"hello\""));
    assert!(combined.contains("\"to\": \"World\""));
    assert!(
        combined.contains("\"to\": \"GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX\"")
    );
    assert!(combined.contains("\"blob\": \"0x0a0b\""));
    assert!(combined.contains("\"blob\": \"0xaabb\""));
    assert!(combined.contains("\"ticker\": \"USDC\""));
    assert!(combined.contains("\"ticker\": \"XLM\""));
    assert!(combined.contains("\"amount\": 100"));
    assert!(combined.contains("\"amount\": 42"));
    assert!(!combined.contains("slot_type_error"));
    assert!(combined.contains("intent_stellar_policy_typed_stage2_normalize.nc"));
}

#[test]
fn example_typed_template_stage3_ok_showcases_practical_normalization() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("intent_stellar_typed_template_stage3_ok.nc");
    if !script_path.exists() {
        eprintln!("skipping test; missing example: {}", script_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(script_path.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar typed template stage3 ok example");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined
            .matches("\"kind\": \"soroban_contract_invoke\"")
            .count()
            >= 3
    );
    assert!(combined.contains("\"to\": \"World\""));
    assert!(
        combined.contains("\"to\": \"GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX\"")
    );
    assert!(combined.contains("\"blob\": \"0xdeadbeef\""));
    assert!(combined.contains("\"blob\": \"0xaabb\""));
    assert!(combined.contains("\"ticker\": \"USDC\""));
    assert!(combined.contains("\"ticker\": \"XLM\""));
    assert!(combined.contains("\"amount\": 1000000"));
    assert!(combined.contains("\"amount\": 42"));
    assert!(!combined.contains("slot_type_error"));
    assert!(combined.contains("intent_stellar_typed_template_stage3_ok.nc"));
}

#[test]
fn example_typed_template_stage3_error_reports_multiple_typed_args_and_blocks_flow() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("intent_stellar_typed_template_stage3_error.nc");
    if !script_path.exists() {
        eprintln!("skipping test; missing example: {}", script_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(script_path.to_string_lossy().to_string())
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar typed template stage3 error example");

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
    assert!(combined.contains("intent_stellar_typed_template_stage3_error.nc"));
}

#[test]
fn nc_script_policy_typed_stage2_normalizes_multiple_user_input_variants() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let contract = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let tmp_policy = std::env::temp_dir().join("nc_script_typed_v2_stage2_variants_policy.json");
    let policy = format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["transfer"],
  "args_schema": {{
    "transfer": {{
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

    let cases: Vec<(&str, String, Vec<String>)> = vec![
        (
            "uppercase_prefix_and_whitespace",
            format!(
                "Invoke contract {contract} function transfer args={{\"to\":\"{}\",\"blob\":\"0X0A0B\",\"ticker\":\" USDC \",\"amount\":\"00100\"}}",
                account.to_ascii_lowercase()
            ),
            vec![
                format!("\"to\": \"{account}\""),
                "\"blob\": \"0x0a0b\"".to_string(),
                "\"ticker\": \"USDC\"".to_string(),
                "\"amount\": 100".to_string(),
            ],
        ),
        (
            "bare_hex_and_spaced_u64",
            format!(
                "Invoke contract {contract} function transfer args={{\"to\":\" {} \",\"blob\":\"AABB\",\"ticker\":\" XLM \",\"amount\":\" 42 \"}}",
                account.to_ascii_lowercase()
            ),
            vec![
                format!("\"to\": \"{account}\""),
                "\"blob\": \"0xaabb\"".to_string(),
                "\"ticker\": \"XLM\"".to_string(),
                "\"amount\": 42".to_string(),
            ],
        ),
        (
            "mixed_case_address_symbol_trim",
            format!(
                "Invoke contract {contract} function transfer args={{\"to\":\"{}\",\"blob\":\"0xdeadBEEF\",\"ticker\":\" token \",\"amount\":\"7\"}}",
                account.to_ascii_lowercase()
            ),
            vec![
                format!("\"to\": \"{account}\""),
                "\"blob\": \"0xdeadbeef\"".to_string(),
                "\"ticker\": \"token\"".to_string(),
                "\"amount\": 7".to_string(),
            ],
        ),
    ];

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    for (case_name, prompt, expected_snippets) in cases {
        let tmp_script =
            std::env::temp_dir().join(format!("nc_script_typed_v2_stage2_{case_name}.nc"));
        let script = format!(
            "AI: \"models/intent_stellar/model.onnx\"\nintent_threshold: 0.00\ncontract_policy: {}\nset stellar intent from AI: \"{}\"\n",
            tmp_policy.to_string_lossy(),
            prompt
        );
        fs::write(&tmp_script, script).expect("write temp nc script");

        let output = Command::new(bin)
            .arg(tmp_script.to_string_lossy().to_string())
            .output()
            .expect("run neurochain-stellar stage2 normalization variant");

        let _ = fs::remove_file(&tmp_script);

        assert!(
            output.status.success(),
            "case {case_name} failed with status {:?}",
            output.status.code()
        );

        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            combined.contains("\"kind\": \"soroban_contract_invoke\""),
            "case {case_name} did not produce soroban_contract_invoke\n{combined}"
        );
        assert!(
            !combined.contains("slot_type_error"),
            "case {case_name} unexpectedly emitted slot_type_error\n{combined}"
        );
        for snippet in expected_snippets {
            assert!(
                combined.contains(&snippet),
                "case {case_name} missing normalized snippet `{snippet}`\n{combined}"
            );
        }
    }

    let _ = fs::remove_file(&tmp_policy);
}

#[test]
fn nc_script_allowlist_settings_can_enforce_without_env() {
    let tmp = std::env::temp_dir().join("nc_script_allowlist_enforce.nc");
    let script = r#"
asset_allowlist: USDC:GISSUER
allowlist_enforce
stellar.payment to="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" amount="5" asset_code="XLM"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(3));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Allowlist violations (enforced)"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_policy_settings_can_enforce_without_env() {
    let policy_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join("CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ")
        .join("policy.json");
    if !policy_path.exists() {
        eprintln!("skipping test; missing policy: {}", policy_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_policy_settings_enforce.nc");
    let script = format!(
        "contract_policy: {}\ncontract_policy_enforce\nsoroban.contract.invoke contract_id=\"CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ\" function=\"hello\"\n",
        policy_path.to_string_lossy()
    );
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar script mode with policy settings");

    assert_eq!(output.status.code(), Some(4));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Contract policy violations (enforced):"));
    assert!(combined.contains("- contract_policy:"));
    assert!(combined.contains("- contract_policy_enforce: on"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_intent_safety_blocks_flow_with_exit_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_intent_safety_block.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
set stellar intent from AI: "Tell me a joke about stars"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.99")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Intent safety guard blocked flow"));
    assert!(combined.contains("\"kind\": \"unknown\""));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_deploy_intent_phase1_builds_deploy_action() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_deploy_intent_phase1_ok.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
set stellar intent from AI: "Invoke deploy contract alias hello-demo wasm ./contracts/hello.wasm"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .output()
        .expect("run neurochain-stellar script mode");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("\"kind\": \"soroban_contract_deploy\""));
    assert!(combined.contains("\"alias\": \"hello-demo\""));
    assert!(combined.contains("\"wasm\": \"./contracts/hello.wasm\""));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_deploy_intent_phase1_missing_wasm_blocks_flow_with_exit_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_deploy_intent_phase1_missing_wasm.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
set stellar intent from AI: "Invoke deploy contract alias hello-demo"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_missing"));
    assert!(combined.contains("ContractDeploy missing wasm"));
    assert!(combined.contains("Intent safety guard blocked flow"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_policy_enforced_blocks_with_exit_4() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
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

    let tmp = std::env::temp_dir().join("nc_script_policy_enforced_block.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .env(
            "NC_CONTRACT_POLICY",
            policy_path.to_string_lossy().to_string(),
        )
        .env("NC_CONTRACT_POLICY_ENFORCE", "1")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(4));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Contract policy violations (enforced):"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_policy_typed_slot_error_blocks_flow_with_exit_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
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

    let tmp = std::env::temp_dir().join("nc_script_policy_typed_slot_error_block.nc");
    let script = format!(
        "contract_policy: {}\nAI: \"models/intent_stellar/model.onnx\"\nset stellar intent from AI: \"Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={{\"to\":\"Hello World\"}}\"\n",
        policy_path.to_string_lossy()
    );
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_type_error"));
    assert!(combined.contains("Intent safety guard blocked flow"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_typed_slot_error_blocks_flow_with_exit_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    let tmp = std::env::temp_dir().join("nc_script_typed_slot_error_block.nc");
    let script = r#"
AI: "models/intent_stellar/model.onnx"
set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":"World"} arg_types={"to":"address"}"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .arg("--intent-threshold")
        .arg("0.00")
        .arg("--flow")
        .arg("--yes")
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(5));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_type_error"));
    assert!(combined.contains("Intent safety guard blocked flow"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_rejects_macro_from_ai_in_stellar_mode() {
    let tmp = std::env::temp_dir().join("nc_script_macro_from_ai_rejected.nc");
    let script = r#"
AI: "models/intent_macro/model.onnx"
macro from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(1));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("macro from AI is not supported in neurochain-stellar"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_set_var_from_ai_fails_fast_without_fallback() {
    let tmp = std::env::temp_dir().join("nc_script_set_var_failfast.nc");
    let script = r#"
AI: "models/does_not_exist/model.onnx"
set mood from AI: "This is wonderful!"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar script mode");

    assert_eq!(output.status.code(), Some(1));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("set_from_ai_failed: variable `mood`"));
    assert!(!combined.contains("raw prompt fallback"));
    assert!(!combined.contains("set_from_ai_fallback"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_x402_request_and_finalize_adds_typed_payment_action() {
    let tmp = std::env::temp_dir().join("nc_script_x402_finalize_ok.nc");
    let script = r#"
x402
x402.request to="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" amount="1" asset_code="XLM"
x402.finalize challenge_id="last"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar x402 script");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("x402 challenge created: x402c0001"));
    assert!(combined.contains("x402 finalize queued: challenge x402c0001"));
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("\"asset_code\": \"XLM\""));
    assert!(combined.contains("- x402: on"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn nc_script_x402_replay_finalize_is_blocked() {
    let tmp = std::env::temp_dir().join("nc_script_x402_finalize_replay_block.nc");
    let script = r#"
x402
x402.request to="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" amount="1" asset_code="XLM"
x402.finalize challenge_id="last"
x402.finalize challenge_id="last"
"#;
    fs::write(&tmp, script).expect("write temp nc script");

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(tmp.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar x402 replay script");

    assert_eq!(output.status.code(), Some(1));

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("x402 finalize blocked: challenge `x402c0001` already finalized"));

    let _ = fs::remove_file(tmp);
}

#[test]
fn example_dorahacks_x402_lite_flow_builds_payment_action() {
    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("_private")
        .join("dorahacks_x402_lite_flow.nc");
    if !script_path.exists() {
        eprintln!("skipping test; missing example: {}", script_path.display());
        return;
    }

    let bin = env!("CARGO_BIN_EXE_neurochain-stellar");
    let output = Command::new(bin)
        .arg(script_path.to_string_lossy().to_string())
        .output()
        .expect("run neurochain-stellar x402 example");

    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("dorahacks_x402_lite_flow.nc"));
    assert!(combined.contains("x402 challenge created: x402c0001"));
    assert!(combined.contains("x402 finalize queued: challenge x402c0001"));
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
}
