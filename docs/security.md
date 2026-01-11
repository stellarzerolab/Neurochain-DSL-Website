# 🛡️ NeuroChain Rust Security & Tooling Stack

Goal: keep the toolchain lean (native-first) while keeping the process strict. Less plugin sprawl, more repeatable commands and CI gates.

## 1. Development Environment (VS Code)
- Required: `rust-analyzer` (set “Check On Save” = `clippy` if possible).
- Recommended: `Even Better TOML` (for `Cargo.toml` editing).
- Optional: Snyk or another polyglot scanner if your organization already uses it – it does not replace CI-level checks.

## 2. Local Workflow (The Local Loop)
Install audit once:
```bash
cargo install cargo-audit
```
Before committing:
```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo audit --deny warnings --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0119
```

Note: `cargo test` includes AI model smoke tests (`src/ai/model/tests.rs`). These tests auto-skip if the referenced ONNX files are missing (useful if you clone without `models/`). For end-to-end validation, run the example scripts that load models (see `docs/getting_started.md` and `examples/`).

## 3. CI/CD Gatekeepers (GitHub Actions Example)
Keep audit as a separate job; combining fmt+clippy saves time.

```yaml
name: Security & Quality

on: [push, pull_request]

jobs:
  lint-fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: rustup component add clippy rustfmt
      - name: Format
        run: cargo fmt --check
      - name: Clippy
        run: cargo clippy -- -D warnings

  tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test

  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install cargo-audit
        run: cargo install cargo-audit
      - name: Run Audit
        run: |
          # Known unmaintained warnings via transitive deps.
          cargo audit --deny warnings \
            --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0119
```

## 4. Supply Chain Hardening (Later)
- `cargo deny`: license policy, banned crates, duplicate-version checks. Recommendation: enable a baseline config (`licenses` + `bans` + `sources` + `duplicates`) for critical parts.

## Summary
1) Editor: `rust-analyzer` warns while you type.  
2) Dev: run `fmt + clippy + test + audit` before pushing.  
3) CI: enforce the same gates to block vulnerable/warning builds.  
4) Growing project: add `cargo deny` for supply-chain hardening.

```
# Install tools (once):
rustup component add clippy rustfmt
cargo install cargo-audit

# Same set as CI runs:
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo audit --deny warnings --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0119
```
