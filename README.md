# NeuroChain DSL

NeuroChain is an **offline**, **deterministic** DSL that combines:

- a small scripting language (`.nc`)
- local CPU ONNX classifiers (`tract-onnx`)
- and an intent-based macro system (`macro from AI: ...`) that turns clear English prompts into deterministic DSL templates

> This repository is the production codebase behind https://stellarzerolab.art.  
> It includes the NeuroChain engine + CLI **and** our website demos (WebUI + Snake) plus the REST API server they use.
>
> The key difference vs an “engine-only” repo is the split of responsibilities:
> - **Engine**: deterministic DSL execution (preprocessing + legacy compatibility + panic-safe execution).
> - **Server**: production wrapper around the engine (HTTP API, concurrency/per-IP limits, optional API key, CORS).
>
> If you are looking for “just the DSL engine”, it's still here — this repo simply also ships the real integration we run on our server.

NeuroChain has two binaries:

- `neurochain` — CLI interpreter (run scripts + interactive mode)
- `neurochain-server` — REST API server (`POST /api/analyze`)

## Repository layout (website + integration)

- `src/` — NeuroChain DSL engine + CLI
- `src/bin/neurochain-server.rs` — Axum REST API used by the WebUI/Snake demos
- `stellarzerolab.art/` — website root (static pages + `webui.html` + `snake.html`)
- `RUNBOOK.md` — aaPanel/Apache/systemd deployment notes for our host

## Highlights

- Offline CPU inference via ONNX classifiers (`tract-onnx`) — no external APIs required
- Built-in classifier workflows: **SST2**, **Toxicity**, **FactCheck**, **Intent**, and **MacroIntent**
- Deterministic **MacroIntent** pipeline (no GPT/LLM fallback)
- Macro loop counts are clamped to `1..=12` for safety (deterministic output)
- Control flow (`if/elif/else`, `and/or`, comparisons) + variables + arithmetic
- Examples double as regression suites (`examples/`)
- CI gates included: `fmt + clippy + test + audit`

## Mini example (`.nc`)

```nc
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "I love this movie."
# Comparisons are case-insensitive and trim whitespace.
if mood == "positive":
    neuro "Great"

# Switch to the MacroIntent model (intent → deterministic DSL → run).
AI: "models/intent_macro/model.onnx"
macro from AI: Show Ping 3 times
```

## Prerequisites (build from source)

- Install Rust + Cargo (via `rustup`): https://www.rust-lang.org/tools/install
- Models are expected under `models/` by default (see `docs/models.md`).
  - Recommended one-time download: `bash scripts/fetch_models.sh` (or PowerShell: `powershell -ExecutionPolicy Bypass -File scripts/fetch_models.ps1`)
- Windows (MSVC): Visual Studio 2022 Build Tools / Community with **Desktop development with C++** (+ Windows SDK)
- Linux/WSL: `build-essential` + `pkg-config`
- macOS: Xcode Command Line Tools (`xcode-select --install`)

## Quickstart

NeuroChain loads ONNX models from `models/` by default (no internet required). See `docs/models.md`.

### 1) Run per-model examples (SST2 / Toxic / FactCheck / Intent)

```bash
cargo run --release --bin neurochain -- examples/distilbert-sst2check.nc
cargo run --release --bin neurochain -- examples/toxiccheck.nc
cargo run --release --bin neurochain -- examples/factcheck.nc
cargo run --release --bin neurochain -- examples/intentcheck.nc
```

### 2) Run the MacroIntent suite (macro → DSL → run)

```bash
cargo run --release --bin neurochain -- examples/macro_test.nc
```

More macro regressions:

```bash
cargo run --release --bin neurochain -- examples/macro_test_edge.nc
cargo run --release --bin neurochain -- examples/macro_test_robust.nc
cargo run --release --bin neurochain -- examples/macro_test_semantics.nc
cargo run --release --bin neurochain -- examples/macro_test_multimodel.nc
cargo run --release --bin neurochain -- examples/macro_test_random.nc
```

### 3) Run the CLI (interactive)

```bash
cargo run --release --bin neurochain
```

In the interactive CLI, you can type: `help`, `about`, `version`, `exit`.
To see command-line flags, run: `cargo run --release --bin neurochain -- --help`.

### 4) Start the REST server

```bash
cargo run --release --bin neurochain-server
```

If you expose `/api/analyze` publicly, set `NC_API_KEY` and require clients/proxy to send `X-API-Key: ...` (or `Authorization: Bearer ...`).

### 5) Run the same gates as CI (recommended before pushing)

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo audit --deny warnings --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0119
```

If disk usage grows due to many builds (debug/release, multiple binaries, tests), run:

```bash
cargo clean
```

If you build in both PowerShell and WSL, you need to clean in both environments. See `docs/troubleshooting.md`.

## Documentation

- `docs/getting_started.md` — end-to-end: run scripts, CLI, server, tests
- `docs/language.md` — DSL language guide (syntax + semantics)
- `docs/macros.md` — MacroIntent (macro → DSL → run) + best practices
- `docs/models.md` — AI models, labels, and multi-model scripts
- `docs/security.md` — Rust security & tooling stack + CI gates
- `docs/troubleshooting.md` — common issues (WSL/target/env/logs)

## Performance notes (MacroIntent)

To see MacroIntent label + score + per-case latency, run:

```bash
cargo test --release --test intent_macro_golden -- --nocapture
```

For model usage examples, see `docs/models.md` and `examples/*check.nc`.

## License

Apache-2.0. See `LICENSE`.

Redistributions must retain `LICENSE` and `NOTICE`.

Note: the `models/` directory may contain third-party model files with their own licenses.

## Branding / trademarks

The Apache-2.0 license does **not** grant any rights to use the NeuroChain DSL or StellarZeroLab names, logos, or branding to imply endorsement or official affiliation.
If you fork this project, please use your own name and branding for your fork/release.

© 2026 StellarZeroLab
