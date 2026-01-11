# 🧯 NeuroChain – Troubleshooting (Common Issues)

## 1) Why does `target/` grow quickly?

Cargo builds separate artifacts:

- `debug` and `release` are different profiles → separate binaries and dependencies
- different binaries (`neurochain`, `neurochain-server`, tests) → separate build artifacts
- Windows and WSL (Linux) are different targets → they cannot share the same build outputs

So the project can easily grow to multiple gigabytes if you build many combinations.

**Rule of thumb:** if you have built “a bit of everything” (multiple binaries + debug/release + tests) and disk usage starts growing, run `cargo clean`.

### Cleanup

Remove build artifacts from the repo directory:

```bash
cargo clean
```

If you build in both PowerShell and WSL, you need to clean in **both environments** (because they have separate toolchains and caches).

## 2) WSL vs PowerShell – why does the build behave differently?

- PowerShell uses the Windows toolchain (`x86_64-pc-windows-msvc`).
- WSL uses the Linux toolchain (`x86_64-unknown-linux-gnu`).

You cannot “merge” these into one build because binaries and linking differ.

### Tip: Use a separate `CARGO_TARGET_DIR` in WSL

If the DrvFS path (`/mnt/c/...`) causes locking/permission issues or you want to keep WSL builds separate, set this in WSL:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/neurochain"
```

After that, `cargo build/test/run` writes build artifacts into your Linux home directory (usually faster and with fewer permission edge cases).

## 3) Environment Variables (PowerShell vs Bash)

PowerShell:

```powershell
$env:NEUROCHAIN_RAW_LOG="1"
$env:NEUROCHAIN_OUTPUT_LOG="1"
$env:NC_INTENT_THRESHOLD="0.35"
```

Bash (WSL/Linux):

```bash
export NEUROCHAIN_RAW_LOG=1
export NEUROCHAIN_OUTPUT_LOG=1
export NC_INTENT_THRESHOLD=0.35
```

### Environment variable reference

**Server**

- `HOST` (default `127.0.0.1`): bind address
- `PORT` (default `8081`): bind port
- `NC_MAX_INFER` (default `2`): max concurrent inference slots (server uses a semaphore)
- `NC_MODELS_DIR` (default `/opt/neurochain/models`): models root directory for the server
  - Local dev tip: if you run the server from this repo, set `NC_MODELS_DIR=models`
- `NC_API_KEY` (optional): if set, `POST /api/analyze` requires `X-API-Key: ...` (or `Authorization: Bearer ...`) — reverse proxy can inject/override this header

**MacroIntent**

- `NC_INTENT_THRESHOLD` (default `0.35`): minimum classifier score before falling back to deterministic heuristics
- `NC_MACRO_MODEL` / `NC_MACRO_MODEL_PATH`: override macro intent model path (defaults to `models/intent_macro/model.onnx` in the CLI)

**Logging**

- `NEUROCHAIN_OUTPUT_LOG=1`: write `neuro:` output to `logs/run_latest.log`
- `NEUROCHAIN_RAW_LOG=1`: write macro intent + generated DSL details to `logs/macro_raw_latest.log`

## 4) The macro output looks “wrong” (how do I debug?)

Enable raw logging and run the same script again:

```powershell
$env:NEUROCHAIN_RAW_LOG="1"
cargo run --release --bin neurochain -- examples/macro_test_random.nc
```

Then inspect `logs/macro_raw_latest.log`:

- `INTENT`: label + score
- `DSL`: the generated DSL that was actually executed

## 5) `cargo audit` fails on “unmaintained” warnings

`cargo audit --deny warnings` can block your build if a dependency is flagged as “unmaintained”.

Options:

- keep “deny warnings” enabled in CI and add temporary `--ignore RUSTSEC-...` (until upstream updates)
- or run `cargo audit` without `--deny warnings` locally

In NeuroChain this is documented in `README.md` and `docs/security.md`.

## 6) WebUI shows “Failed to fetch” with a local server

- Make sure the server is running and the Base URL is `http://127.0.0.1:8081`.
- If the WebUI page is HTTPS, some browsers may block HTTPS → `http://127.0.0.1` requests. Allow localhost mixed content or try another browser.
