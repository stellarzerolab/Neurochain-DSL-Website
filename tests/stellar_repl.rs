use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use tempfile::TempDir;

fn assert_contains_in_order(haystack: &str, needles: &[&str]) {
    let mut pos = 0usize;
    for needle in needles {
        let Some(offset) = haystack[pos..].find(needle) else {
            panic!("expected to find `{needle}` after byte position {pos}\n{haystack}");
        };
        pos += offset + needle.len();
    }
}

fn help_row(command: &str, description: &str) -> String {
    format!("- {:<58} {}", command, description)
}

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

fn spawn_friendbot_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind friendbot test server");
    let addr = listener
        .local_addr()
        .expect("friendbot test server local addr");
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
    format!("http://{}/friendbot", addr)
}

#[test]
fn stellar_repl_help_and_exit_work() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin("help\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("NeuroChain Stellar REPL"))
        .stdout(contains("Stellar REPL quick start"))
        .stdout(contains("help dsl"))
        .stdout(contains(
            "Toggle commands are listed in `help all` under Toggles (on/off).",
        ))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_starts_with_flow_flag_only() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.arg("--flow")
        .write_stdin("exit\n\n")
        .assert()
        .success()
        .stdout(contains("NeuroChain Stellar REPL"))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_accepts_ai_model_line() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin("AI: \"models/intent_stellar/model.onnx\"\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains(
            "Intent model path set to: models/intent_stellar/model.onnx",
        ))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_accepts_network_and_wallet_commands() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin("set network = \"testnet\"\n\nset wallet = \"nc-testnet\"\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("Network set to: testnet"))
        .stdout(contains("Wallet/source set to: nc-testnet"))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_wallet_generate_creates_alias_and_sets_source() {
    let (_tmp_dir, fake_cli) = create_fake_stellar_cli();
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin(format!(
        "stellar_cli: \"{}\"\n\nwallet_generate: demo-alias\n\nshow setup\n\nexit\n\n",
        fake_cli.to_string_lossy()
    ))
    .assert()
    .success()
    .stdout(contains("Wallet key alias generated: demo-alias"))
    .stdout(contains(
        "Public key/address: GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX",
    ))
    .stdout(contains("Wallet/source set to: demo-alias"))
    .stdout(contains("- wallet/source: demo-alias"));
}

#[test]
fn stellar_repl_wallet_bootstrap_generates_funds_and_sets_source() {
    let (_tmp_dir, fake_cli) = create_fake_stellar_cli();
    let friendbot_url = spawn_friendbot_server();
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin(format!(
        "stellar_cli: \"{}\"\n\nfriendbot: \"{}\"\n\nwallet_bootstrap: demo-boot\n\nshow setup\n\nexit\n\n",
        fake_cli.to_string_lossy(),
        friendbot_url
    ))
    .assert()
    .success()
    .stdout(contains("Wallet key alias generated: demo-boot"))
    .stdout(contains(
        "Public key/address: GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX",
    ))
    .stdout(contains("Friendbot: friendbot funded account"))
    .stdout(contains("Wallet/source set to: demo-boot"))
    .stdout(contains("- wallet/source: demo-boot"));
}

#[test]
fn stellar_repl_accepts_runtime_setting_commands() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin(
        "intent_threshold: 0.60\n\nhorizon: https://horizon-testnet.stellar.org\n\nfriendbot: off\n\nstellar_cli: stellar\n\nsimulate_flag: \"--send no\"\n\ntxrep\n\ntxrep off\n\nx402\n\nx402 off\n\nasset_allowlist: XLM\n\nsoroban_allowlist: CTEST:transfer\n\ncontract_policy: contracts/demo/policy.json\n\ncontract_policy_dir: contracts\n\nallowlist_enforce\n\ncontract_policy_enforce\n\ndebug\n\ndebug off\n\nallowlist_enforce off\n\ncontract_policy_enforce off\n\nallowlist_enforce\n\nexit\n\n",
    )
    .assert()
    .success()
    .stdout(contains("Intent threshold set to: 0.60"))
    .stdout(contains("Horizon URL set to: https://horizon-testnet.stellar.org"))
    .stdout(contains("Friendbot set to: (disabled)"))
    .stdout(contains("Stellar CLI binary set to: stellar"))
    .stdout(contains("Soroban simulate flag set to: --send no"))
    .stdout(contains("Txrep preview: enabled"))
    .stdout(contains("Txrep preview: disabled"))
    .stdout(contains("x402 mode: enabled"))
    .stdout(contains("x402 mode: disabled"))
    .stdout(contains("Asset allowlist set to: XLM"))
    .stdout(contains("Soroban allowlist set to: CTEST:transfer"))
    .stdout(contains("Contract policy file: contracts/demo/policy.json"))
    .stdout(contains("Contract policy dir: contracts"))
    .stdout(contains("Allowlist enforce: enabled"))
    .stdout(contains("Contract policy enforce: enabled"))
    .stdout(contains("Intent debug trace: enabled"))
    .stdout(contains("Intent debug trace: disabled"))
    .stdout(contains("Allowlist enforce: disabled"))
    .stdout(contains("Contract policy enforce: disabled"))
    .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_supports_help_all_show_config_and_setup_testnet() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin(
        "help all\n\nshow config\n\nsetup testnet\n\ntxrep off\n\nshow config\n\nexit\n\n",
    )
    .assert()
    .success()
    .stdout(contains("Stellar REPL commands (all)"))
    .stdout(contains("Current REPL config:"))
    .stdout(contains(
        "Applied testnet baseline (network+horizon+friendbot).",
    ))
    .stdout(contains("Txrep preview: disabled"))
    .stdout(contains("- txrep_preview: off"))
    .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_help_all_is_sectioned_and_single_line_formatted() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin("help all\n\nexit\n\n")
        .output()
        .expect("run help all in repl");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_contains_in_order(
        &stdout,
        &[
            "Stellar REPL commands (all):",
            "Core setup (value required):",
            "Toggles (on/off):",
            "Prompt/Action commands:",
            "Utility commands:",
        ],
    );

    let ai_row = help_row("AI: \"path\"", "set intent model path");
    let threshold_row = help_row("intent_threshold: <f32>", "set intent confidence threshold");
    let network_row = help_row(
        "network: testnet|mainnet|public",
        "set active network for flow",
    );
    let wallet_generate_row = help_row(
        "wallet_generate: <alias>",
        "generate a local stellar key alias",
    );
    let wallet_bootstrap_row = help_row(
        "wallet_bootstrap: <alias>",
        "generate key alias and friendbot-fund it",
    );
    let txrep_row = help_row("txrep", "enable txrep preview in flow");
    let x402_row = help_row("x402", "enable x402-lite flow commands");
    let enforce_row = help_row("allowlist_enforce", "enable allowlist enforce");
    let policy_row = help_row(
        "contract_policy: <path>",
        "set NC_CONTRACT_POLICY equivalent",
    );
    let policy_dir_row = help_row(
        "contract_policy_dir: <dir>",
        "set NC_CONTRACT_POLICY_DIR equivalent",
    );
    let policy_enforce_row = help_row("contract_policy_enforce", "enable contract policy enforce");
    let debug_row = help_row("debug", "enable intent pipeline trace");
    let intent_row = help_row(
        "set stellar intent from AI: \"...\"",
        "classify prompt -> ActionPlan",
    );
    let deploy_row = help_row(
        "soroban.contract.deploy alias=\"...\" wasm=\"...\"",
        "manual deploy action",
    );
    let set_var_row = help_row(
        "set <var> from AI: \"...\"",
        "predict with active model -> store variable",
    );
    let x402_request_row = help_row(
        "x402.request to=\"...\" amount=\"...\" asset_code=\"XLM\"",
        "create x402-lite payment challenge",
    );
    let x402_finalize_row = help_row(
        "x402.finalize challenge_id=\"last\"",
        "finalize challenge -> execute typed stellar_payment",
    );
    let setup_row = help_row("show setup", "print active setup");
    let help_dsl_row = help_row("help dsl", "show normal NeuroChain DSL language help");

    assert!(stdout.contains(&ai_row));
    assert!(stdout.contains(&threshold_row));
    assert!(stdout.contains(&network_row));
    assert!(stdout.contains(&wallet_generate_row));
    assert!(stdout.contains(&wallet_bootstrap_row));
    assert!(stdout.contains(&txrep_row));
    assert!(stdout.contains(&x402_row));
    assert!(stdout.contains(&enforce_row));
    assert!(stdout.contains(&policy_row));
    assert!(stdout.contains(&policy_dir_row));
    assert!(stdout.contains(&policy_enforce_row));
    assert!(stdout.contains(&debug_row));
    assert!(stdout.contains(&set_var_row));
    assert!(stdout.contains(&x402_request_row));
    assert!(stdout.contains(&x402_finalize_row));
    assert!(stdout.contains(&intent_row));
    assert!(stdout.contains(&deploy_row));
    assert!(stdout.contains(&help_dsl_row));
    assert!(stdout.contains(&setup_row));

    let core_start = stdout
        .find("Core setup (value required):")
        .expect("core setup header");
    let toggle_start = stdout.find("Toggles (on/off):").expect("toggle header");
    let prompt_start = stdout
        .find("Prompt/Action commands:")
        .expect("prompt/action header");
    let utility_start = stdout.find("Utility commands:").expect("utility header");

    let core_section = &stdout[core_start..toggle_start];
    let toggle_section = &stdout[toggle_start..prompt_start];
    let prompt_section = &stdout[prompt_start..utility_start];

    assert!(core_section.contains("intent_threshold: <f32>"));
    assert!(!core_section.contains("txrep"));

    assert!(toggle_section.contains("txrep"));
    assert!(toggle_section.contains("x402"));
    assert!(toggle_section.contains("allowlist_enforce"));
    assert!(toggle_section.contains("contract_policy_enforce"));
    assert!(toggle_section.contains("debug"));
    assert!(!toggle_section.contains("intent_threshold: <f32>"));

    assert!(prompt_section.contains("set stellar intent from AI: \"...\""));
    assert!(prompt_section.contains("x402.request to=\"...\" amount=\"...\" asset_code=\"XLM\""));
}

#[test]
fn stellar_repl_x402_request_finalize_and_replay_block_work() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .arg("--no-flow")
        .write_stdin(
            "x402\n\nx402.request to=\"GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\" amount=\"1\" asset_code=\"XLM\"\n\nx402.finalize challenge_id=\"last\"\n\nx402.finalize challenge_id=\"last\"\n\nexit\n\n",
        )
        .output()
        .expect("run repl x402 lite flow");
    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("x402 mode: enabled"));
    assert!(combined.contains("x402 challenge created: x402c0001"));
    assert!(combined.contains("x402 finalize: challenge `x402c0001`"));
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
    assert!(combined.contains("x402 finalize blocked: challenge `x402c0001` already finalized"));
}

#[test]
fn stellar_repl_policy_settings_can_enforce_without_env() {
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

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin(format!(
            "contract_policy: {}\n\ncontract_policy_enforce\n\nAI: \"models/intent_stellar/model.onnx\"\n\nintent_threshold: 0.00\n\nset stellar intent from AI: \"Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello\"\n\nexit\n\n",
            policy_path.to_string_lossy()
        ))
        .output()
        .expect("run repl policy settings without env");
    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Contract policy file:"));
    assert!(combined.contains("Contract policy enforce: enabled"));
    assert!(combined.contains("Contract policy violations (enforced):"));
    assert!(combined.contains("repl step returned code 4"));
}

#[test]
fn stellar_repl_set_var_from_ai_does_not_trigger_intent_flow() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin(
            "AI: \"models/intent_stellar/model.onnx\"\n\nset mood from AI: \"Send 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\"\n\nexit\n\n",
        )
        .output()
        .expect("run repl set var from ai");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("Variable mood set from AI:"));
    assert!(!stdout.contains("\"schema_version\""));
    assert!(!stderr.contains("=== Preview ==="));
}

#[test]
fn stellar_repl_set_var_from_ai_fails_fast_without_raw_fallback() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin(
            "AI: \"models/does_not_exist/model.onnx\"\n\nset mood from AI: \"This is wonderful!\"\n\nexit\n\n",
        )
        .output()
        .expect("run repl set var from ai failfast");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("set_from_ai_failed mood:"));
    assert!(!stdout.contains("Variable mood set from AI:"));
    assert!(!stdout.contains("raw prompt fallback"));
}

#[test]
fn stellar_repl_macro_from_ai_is_rejected_with_guidance() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin("macro from AI: \"Transfer 5 XLM to G...\"\n\nexit\n\n")
        .output()
        .expect("run repl macro from ai");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(
        "macro from AI is not supported in neurochain-stellar; use set stellar intent from AI"
    ));
    assert!(!stdout.contains("\"schema_version\""));
    assert!(!stderr.contains("=== Preview ==="));
}

#[test]
fn stellar_repl_defaults_to_flow_mode_without_flag() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.env_remove("NC_ALLOWLIST_ENFORCE")
        .env_remove("NC_CONTRACT_POLICY_ENFORCE")
        .env_remove("NC_ASSET_ALLOWLIST")
        .env_remove("NC_SOROBAN_ALLOWLIST")
        .write_stdin("stellar.payment to=\"GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\" amount=\"1\" asset_code=\"XLM\"\n\nn\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("Flow mode: enabled"))
        .stderr(contains("=== Preview ==="))
        .stderr(contains("Confirm submit? [y/N]"))
        .stderr(contains("Submit aborted by user."));
}

#[test]
fn stellar_repl_no_flow_flag_disables_preview() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.arg("--no-flow")
        .write_stdin("stellar.payment to=\"GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\" amount=\"1\" asset_code=\"XLM\"\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("Flow mode: disabled"))
        .stdout(contains("=== Preview ===").not());
}

#[test]
fn stellar_repl_starts_without_wallet_even_when_env_source_is_set() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.env("NC_SOROBAN_SOURCE", "nc-testnet")
        .write_stdin("exit\n\n")
        .assert()
        .success()
        .stdout(contains("Current wallet/source: (not set)"));
}

#[test]
fn stellar_repl_defaults_asset_allowlist_to_xlm() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.env_remove("NC_ASSET_ALLOWLIST")
        .write_stdin("exit\n\n")
        .assert()
        .success()
        .stdout(contains("Current asset_allowlist: XLM"));
}

#[test]
fn stellar_repl_help_dsl_shows_language_help() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.write_stdin("help dsl\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("NeuroChain language"))
        .stdout(contains("Basic syntax:"))
        .stdout(contains("AI: \"path/to/model.onnx\""))
        .stdout(contains("Exiting"));
}

#[test]
fn stellar_repl_intent_safety_block_reports_step_code_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin(
            "AI: \"models/intent_stellar/model.onnx\"\n\nintent_threshold: 0.99\n\nset stellar intent from AI: \"Tell me a joke about stars\"\n\nexit\n\n",
        )
        .output()
        .expect("run repl low-confidence guard");
    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Intent safety guard blocked flow"));
    assert!(combined.contains("repl step returned code 5"));
}

#[test]
fn stellar_repl_debug_emits_intent_trace_lines() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    cmd.arg("--no-flow");
    let output = cmd
        .write_stdin(
            "debug\n\nAI: \"models/intent_stellar/model.onnx\"\n\nintent_threshold: 0.20\n\nset stellar intent from AI: \"Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\"\n\nexit\n\n",
        )
        .output()
        .expect("run repl debug trace");
    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Intent debug trace: enabled"));
    assert!(combined.contains("[intent-debug]"));
    assert!(combined.contains("\"kind\": \"stellar_payment\""));
}

#[test]
fn stellar_repl_allowlist_enforced_reports_step_code_3() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin(
            "asset_allowlist: USDC:GISSUER\n\nallowlist_enforce\n\nAI: \"models/intent_stellar/model.onnx\"\n\nintent_threshold: 0.20\n\nset stellar intent from AI: \"Send 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P\"\n\nexit\n\n",
        )
        .output()
        .expect("run repl allowlist enforce");
    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Allowlist violations (enforced):"));
    assert!(combined.contains("repl step returned code 3"));
}

#[test]
fn stellar_repl_policy_enforced_reports_step_code_4() {
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

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .env("NC_CONTRACT_POLICY", policy_path.to_string_lossy().to_string())
        .env("NC_CONTRACT_POLICY_ENFORCE", "1")
        .write_stdin(
            "AI: \"models/intent_stellar/model.onnx\"\n\nintent_threshold: 0.00\n\nset stellar intent from AI: \"Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello\"\n\nexit\n\n",
        )
        .output()
        .expect("run repl policy enforce");
    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Contract policy violations (enforced):"));
    assert!(combined.contains("repl step returned code 4"));
}

#[test]
fn stellar_repl_policy_typed_slot_error_reports_step_code_5() {
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

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin(format!(
            "contract_policy: {}\n\nAI: \"models/intent_stellar/model.onnx\"\n\nintent_threshold: 0.00\n\nset stellar intent from AI: \"Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={{\"to\":\"Hello World\"}}\"\n\nexit\n\n",
            policy_path.to_string_lossy()
        ))
        .output()
        .expect("run repl policy typed-slot type error");
    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_type_error"));
    assert!(combined.contains("repl step returned code 5"));
}

#[test]
fn stellar_repl_typed_slot_error_reports_step_code_5() {
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("intent_stellar")
        .join("model.onnx");
    if !model_path.exists() {
        eprintln!("skipping test; missing model: {}", model_path.display());
        return;
    }

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain-stellar").expect("bin build");
    let output = cmd
        .write_stdin(
            r#"AI: "models/intent_stellar/model.onnx"

intent_threshold: 0.00

Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":"World"} arg_types={"to":"address"}

exit

"#,
        )
        .output()
        .expect("run repl typed-slot type error");
    assert!(output.status.success());

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("slot_type_error"));
    assert!(combined.contains("repl step returned code 5"));
}
