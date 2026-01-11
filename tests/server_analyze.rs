use std::{
    io::ErrorKind,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct AnalyzeResp {
    ok: bool,
    output: String,
    logs: Vec<String>,
}

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

fn macro_model_path() -> PathBuf {
    let base = std::env::var("NC_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| models_dir());

    base.join("intent_macro").join("model.onnx")
}

#[test]
fn api_analyze_smoke_and_errors() {
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

    // Ensure cleanup even if the test fails.
    let _server = Server { child };

    wait_for_listen(addr, Duration::from_secs(3));

    // 1) Empty input -> ok=false
    let body = json!({"model":"macro","content":""}).to_string();
    let (status, resp_body) = http_post_json(addr, "/api/analyze", &body);
    assert_eq!(status, 200);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(!resp.ok, "empty input should return ok=false");

    // 2) Unknown model id should warn but still run simple scripts
    let body = json!({"model":"unknown","content":"neuro \"hi\""}).to_string();
    let (status, resp_body) = http_post_json(addr, "/api/analyze", &body);
    assert_eq!(status, 200);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(resp.ok, "unknown model should not break non-AI scripts");
    assert!(
        resp.logs
            .iter()
            .any(|l| l.contains("warn: unknown model id")),
        "expected warn log for unknown model id"
    );

    // 3) Known model id should auto-inject AI model path when missing
    let macro_model = macro_model_path();
    if !macro_model.exists() {
        eprintln!(
            "api_analyze_smoke_and_errors skipped: model not found at {}",
            macro_model.display()
        );
        return;
    }

    let body = json!({"model":"macro","content":"neuro \"hi\""}).to_string();
    let (status, resp_body) = http_post_json(addr, "/api/analyze", &body);
    assert_eq!(status, 200);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(resp.ok);
    assert!(
        resp.logs
            .iter()
            .any(|l| l.contains("auto: injected AI model path")),
        "expected auto-injection log"
    );
    assert!(
        resp.logs
            .iter()
            .any(|l| l.contains("intent_macro/model.onnx")),
        "expected injected macro model path in logs"
    );

    // Keep the assertion loose to avoid coupling to exact formatting.
    assert!(
        !resp.output.trim().is_empty(),
        "server output should not be empty"
    );
}

#[test]
fn api_analyze_requires_api_key_when_configured() {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let api_key = "test-key-123";

    let child = Command::new(assert_cmd::cargo::cargo_bin!("neurochain-server"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("NC_MODELS_DIR", models_dir())
        .env("NC_API_KEY", api_key)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn neurochain-server");

    let _server = Server { child };

    wait_for_listen(addr, Duration::from_secs(3));

    let body = json!({"model":"unknown","content":"neuro \"hi\""}).to_string();

    // 1) Missing key -> 401
    let (status, resp_body) = http_post_json(addr, "/api/analyze", &body);
    assert_eq!(status, 401);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(!resp.ok);

    // 2) With key -> 200
    let (status, resp_body) =
        http_post_json_with_headers(addr, "/api/analyze", &body, &[("X-API-Key", api_key)]);
    assert_eq!(status, 200);
    let resp: AnalyzeResp = serde_json::from_str(&resp_body).expect("json parse");
    assert!(resp.ok);
    assert!(resp.output.contains("hi"));
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
