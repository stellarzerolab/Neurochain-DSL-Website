use std::{
    fs,
    io::ErrorKind,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::json;

struct Server {
    child: Child,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn find_free_port() -> u16 {
    // Bind to port 0 to let the OS pick a free port, then release it.
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

fn wait_for_listen(addr: SocketAddr, timeout: Duration) {
    let start = Instant::now();
    loop {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return;
        }
        if start.elapsed() > timeout {
            panic!("server did not start listening on {addr} within {timeout:?}");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn http_post_json(addr: SocketAddr, path: &str, json_body: &str) -> (u16, String) {
    http_post_json_with_headers(addr, path, json_body, &[])
}

fn http_post_json_with_headers(
    addr: SocketAddr,
    path: &str,
    json_body: &str,
    headers: &[(&str, &str)],
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set_read_timeout");

    let extra_headers = headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect::<String>();

    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{extra}Connection: close\r\nContent-Length: {len}\r\n\r\n{body}",
        host = addr,
        len = json_body.len(),
        body = json_body,
        extra = extra_headers
    );

    stream.write_all(req.as_bytes()).expect("write request");

    // Read headers first, then read exactly Content-Length bytes (do not rely on EOF).
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let start = Instant::now();
    let header_end = loop {
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if start.elapsed() > Duration::from_secs(30) {
                    panic!("timeout waiting for response headers from {addr}");
                }
                continue;
            }
            Err(e) => panic!("read response: {e}"),
        };
        if n == 0 {
            panic!("unexpected EOF while reading headers");
        }
        buf.extend_from_slice(&chunk[..n]);

        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break (pos, 4usize);
        }
        if let Some(pos) = find_subsequence(&buf, b"\n\n") {
            break (pos, 2usize);
        }
        if buf.len() > 64 * 1024 {
            panic!("headers too large");
        }
    };

    let (header_pos, header_len) = header_end;
    let split_at = header_pos + header_len;
    let (head_bytes, body_bytes) = buf.split_at(split_at);
    let head_str = String::from_utf8_lossy(head_bytes);

    // Split headers/body
    let status_line = head_str.lines().next().unwrap_or_default();
    let mut parts = status_line.split_whitespace();
    let _http = parts.next().unwrap_or_default();
    let code = parts
        .next()
        .unwrap_or_default()
        .parse::<u16>()
        .expect("status code");

    let content_len = head_str
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);

    let mut body: Vec<u8> = body_bytes.to_vec();
    if content_len == 0 {
        // Fallback: read until EOF (Connection: close).
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&chunk[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    if start.elapsed() > Duration::from_secs(30) {
                        panic!("timeout waiting for response body from {addr}");
                    }
                    continue;
                }
                Err(e) => panic!("read body: {e}"),
            };
        }
    } else {
        while body.len() < content_len {
            let n = match stream.read(&mut chunk) {
                Ok(n) => n,
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    if start.elapsed() > Duration::from_secs(30) {
                        panic!("timeout waiting for response body from {addr}");
                    }
                    continue;
                }
                Err(e) => panic!("read body: {e}"),
            };
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        body.truncate(content_len);
    }

    let body_str = String::from_utf8_lossy(&body).to_string();
    (code, body_str)
}

fn models_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("models");
    path
}

fn intent_stellar_model_path() -> PathBuf {
    let base = std::env::var("NC_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| models_dir());

    base.join("intent_stellar").join("model.onnx")
}

#[test]
fn api_stellar_intent_plan_smoke_and_blocks() {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let child = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-server"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("NC_MODELS_DIR", models_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn neurochain-server");

    let _server = Server { child };

    wait_for_listen(addr, Duration::from_secs(3));

    let model = intent_stellar_model_path();
    if !model.exists() {
        eprintln!(
            "api_stellar_intent_plan_smoke_and_blocks skipped: model not found at {}",
            model.display()
        );
        return;
    }

    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let body = json!({
        "model": "intent_stellar",
        "prompt": format!("Check balance for {account} asset XLM"),
        "threshold": 0.0
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "stellar_account_balance"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Tell me a joke about stars",
        "threshold": 0.99
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    assert_eq!(resp["plan"]["actions"][0]["kind"], "unknown");

    let body = json!({
        "model": "intent_stellar",
        "prompt": format!("Send 5 XLM to {account}"),
        "threshold": 0.20,
        "allowlist_assets": "USDC:GISSUER",
        "allowlist_enforce": true
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 3);
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("allowlist: violations=")),
        "expected allowlist summary in logs"
    );
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l == "block: allowlist_enforced"),
        "expected allowlist block marker in logs"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello",
        "threshold": 0.00,
        "contract_policy_enforce": true
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 4);

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\" World \"}",
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "soroban_contract_invoke"
    );
    assert_eq!(resp["plan"]["actions"][0]["args"]["to"], "World");
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("typed_template_v2:") && l.contains("normalized_args=")),
        "expected typed_template_v2 normalized_args summary in logs"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"World\"} arg_types={\"to\":\"address\"}",
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("typed_template_v2: policy_slot_type_converted=")),
        "expected typed_template_v2 summary in logs"
    );
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l == "block: intent_safety"),
        "expected intent safety block marker in logs"
    );
    let warnings = resp["plan"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .filter_map(|v| v.as_str())
            .any(|w| w.contains("slot_type_error")),
        "expected slot_type_error warning"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"Hello World\"}",
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    let warnings = resp["plan"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .filter_map(|v| v.as_str())
            .any(|w| w.contains("slot_type_error") && w.contains("policy")),
        "expected policy-derived slot_type_error warning"
    );
}

#[test]
fn api_stellar_intent_plan_stage3_typed_v2_parity() {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    let model = intent_stellar_model_path();
    if !model.exists() {
        eprintln!(
            "api_stellar_intent_plan_stage3_typed_v2_parity skipped: model not found at {}",
            model.display()
        );
        return;
    }

    let contract = "CELFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy_path = std::env::temp_dir().join("nc_server_api_stage3_typed_v2_policy.json");
    let policy_dir = std::env::temp_dir().join("nc_server_api_stage3_empty_policies");
    let _ = fs::create_dir_all(&policy_dir);
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
    fs::write(&policy_path, policy).expect("write temp stage3 policy");

    let child = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-server"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("NC_MODELS_DIR", models_dir())
        .env(
            "NC_CONTRACT_POLICY",
            policy_path.to_string_lossy().to_string(),
        )
        .env(
            "NC_CONTRACT_POLICY_DIR",
            policy_dir.to_string_lossy().to_string(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn neurochain-server");
    let _server = Server { child };

    wait_for_listen(addr, Duration::from_secs(3));

    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let body = json!({
        "model": "intent_stellar",
        "prompt": format!(
            "Invoke contract {contract} function hello args={{\"to\":\" {} \",\"blob\":\"0XDE AD_be-EF\",\"ticker\":\" USDC \",\"amount\":\"1_000,000\"}}",
            account.to_ascii_lowercase()
        ),
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["blocked"], false);
    assert_eq!(
        resp["plan"]["actions"][0]["kind"],
        "soroban_contract_invoke"
    );
    assert_eq!(resp["plan"]["actions"][0]["args"]["to"], account);
    assert_eq!(resp["plan"]["actions"][0]["args"]["blob"], "0xdeadbeef");
    assert_eq!(resp["plan"]["actions"][0]["args"]["ticker"], "USDC");
    assert_eq!(resp["plan"]["actions"][0]["args"]["amount"], 1000000);
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("typed_template_v2:") && l.contains("normalized_args=")),
        "expected typed_template_v2 normalized_args summary in logs"
    );

    let body = json!({
        "model": "intent_stellar",
        "prompt": format!(
            "Invoke contract {contract} function hello args={{\"to\":\"World\",\"blob\":\"0xABC\",\"ticker\":\" BAD VALUE \",\"amount\":\"18446744073709551616\"}}"
        ),
        "threshold": 0.00
    })
    .to_string();
    let (status, resp_body) = http_post_json(addr, "/api/stellar/intent-plan", &body);
    assert_eq!(status, 200);

    let resp: serde_json::Value = serde_json::from_str(&resp_body).expect("json parse");
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["blocked"], true);
    assert_eq!(resp["exit_code"], 5);
    assert_eq!(resp["plan"]["actions"][0]["kind"], "unknown");
    let warnings = resp["plan"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let warning_text = warnings
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(warning_text.contains("slot_type_error"));
    assert!(warning_text.contains("ContractInvoke to"));
    assert!(warning_text.contains("ContractInvoke blob"));
    assert!(warning_text.contains("ContractInvoke ticker"));
    assert!(warning_text.contains("ContractInvoke amount"));
    let logs = resp["logs"].as_array().cloned().unwrap_or_default();
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l.contains("typed_template_v2: policy_slot_type_converted=1")),
        "expected typed_template_v2 conversion summary in logs"
    );
    assert!(
        logs.iter()
            .filter_map(|v| v.as_str())
            .any(|l| l == "block: intent_safety"),
        "expected intent safety block marker in logs"
    );

    let _ = fs::remove_file(&policy_path);
    let _ = fs::remove_dir(&policy_dir);
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
