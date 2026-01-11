# 🚀 NeuroChain – Getting Started (End-to-End)

This is a short “do this” guide to run a NeuroChain script from start to finish. The goal is to get a quick feel for the language: `neuro`, `set`, arithmetic, and `if/else`.

## 0) Prerequisites

- Rust + Cargo installed (via `rustup`).
- Models available under `models/` (this repo uses example paths).
  - If you cloned without models, download them once using:
    - `bash scripts/fetch_models.sh` (Linux / macOS / WSL)
    - `powershell -ExecutionPolicy Bypass -File scripts/fetch_models.ps1` (Windows PowerShell)
- OS build tooling (required for native dependencies):
  - Windows (MSVC): Visual Studio 2022 Build Tools / Community with **Desktop development with C++** (+ Windows SDK)
  - Linux/WSL: `build-essential` + `pkg-config`
  - macOS: Xcode Command Line Tools (`xcode-select --install`)

## 1) Run a script

NeuroChain can be run in two build modes:

- **Debug** (faster builds, slower runtime):
  ```bash
  cargo run --bin neurochain -- examples/macro_test.nc
  ```
- **Release** (slower builds, faster runtime):
  ```bash
  cargo run --release --bin neurochain -- examples/macro_test.nc
  ```

Most examples in this repo use `--release` so macro/model runs are fast.

You can also run your own file (e.g. `my_script.nc`):

```bash
cargo run --release --bin neurochain -- my_script.nc
```

## 2) Example: Hello + variables + if

Save this as `my_script.nc`:

```nc
# 1) Hello
neuro "Hello from NeuroChain"

// 2) Variables + arithmetic
set x = 5
set y = 3
set sum = x + y
neuro sum

// 3) Condition
if sum >= 8:
    neuro "Big"
else:
    neuro "Small"

// 4) String concatenation
set name = "Joe"
set greeting = "Hello, " + name
neuro greeting
```

Run it:

```bash
cargo run --release --bin neurochain -- my_script.nc
```

Example output (after the banner):

```text
neuro: Hello from NeuroChain
neuro: 8
neuro: Big
neuro: Hello, Joe
```

## 3) Interactive CLI

Start without a file:

```bash
cargo run --bin neurochain
```

Meta commands inside the interactive CLI:

```text
help
about
version
exit
```

Note: these are **interactive** commands (type them after starting `neurochain` without a file).
If you run `neurochain about` / `neurochain version` as arguments, they will be treated as a script filename.

One-shot equivalents:

```bash
cargo run --release --bin neurochain -- help
cargo run --release --bin neurochain -- --about
cargo run --release --bin neurochain -- --version
```

Tip: the extra `--` is required to pass flags through `cargo run`.

### If you want to test the server with WebUI

1) Start the server:

```bash
cargo run --release --bin neurochain-server
```

2) Open `https://stellarzerolab.art/webui` in the browser.

3) In WebUI:

- Runtime → **API (local / same-origin)**
- API Base URL → `http://127.0.0.1:8081`

Note: `127.0.0.1` always points to your own machine, so the WebUI will call your local server (not the public site).

4) Press **Run** → output should appear.

If you see “Failed to fetch”, make sure the server is running, the Base URL is correct, and your browser allows HTTPS → `http://127.0.0.1` requests.


## 4) Run the REST server (optional)

Start the API server:

- **Debug**:
  ```bash
  cargo run --bin neurochain-server
  ```
- **Release**:
  ```bash
  cargo run --release --bin neurochain-server
  ```

Defaults:
- `HOST=127.0.0.1`
- `PORT=8081`
- Endpoint: `POST /api/analyze`

Optional auth:
- `NC_API_KEY=...`: if set, requests must include `X-API-Key: ...` (or `Authorization: Bearer ...`).
  Note: the hosted WebUI does not send an API key by default, so use curl/your own client or leave it unset for local tests.

Quick test (SST‑2):

```bash
curl -s http://127.0.0.1:8081/api/analyze \
  -H "Content-Type: application/json" \
  -d '{"model":"sst2","content":"set mood from AI: \"This is amazing!\"\nif mood == \"Positive\":\n    neuro \"Great\"\nelse:\n    neuro \"Bad\""}'
```

Note: if your payload does not contain an `AI:` line, the server injects the model path automatically based on the `model` field (see `docs/models.md`).

## 5) Debug: enable logs (optional)

PowerShell:

```powershell
$env:NEUROCHAIN_RAW_LOG="1"
$env:NEUROCHAIN_OUTPUT_LOG="1"
cargo run --release --bin neurochain -- examples/macro_test.nc
```

Files:

- `logs/macro_raw_latest.log` (macro → intent + generated DSL)
- `logs/run_latest.log` (all `neuro:` output)

## 6) Run tests (optional)

This repo includes unit + integration tests. To run the same gates as CI:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Useful focused tests:

```bash
cargo test --test cli_meta
cargo test --test dsl_semantics
cargo test --test server_analyze
cargo test --test intent_macro_golden -- --nocapture
```

Notes:

- `server_analyze` spawns a local `neurochain-server` on a free port and calls `POST /api/analyze`.
- Some tests assume models exist under `models/` (or use `NC_MODELS_DIR`).

## 7) Run example suites (recommended)

These scripts act as end-to-end regression suites:

- Macro suites:
  - `examples/macro_test.nc`
  - `examples/macro_test_edge.nc`
  - `examples/macro_test_robust.nc`
  - `examples/macro_test_semantics.nc`
  - `examples/macro_test_multimodel.nc`
  - `examples/macro_test_random.nc`
- Per-model scripts:
  - `examples/distilbert-sst2check.nc`
  - `examples/toxiccheck.nc`
  - `examples/factcheck.nc`
  - `examples/intentcheck.nc`

## 8) See also

- `docs/language.md` – DSL syntax basics
- `docs/macros.md` – MacroIntent (macro → DSL → run)
- `docs/models.md` – AI models and `set x from AI:` usage
- `docs/troubleshooting.md` – common issues (WSL/target/env)
