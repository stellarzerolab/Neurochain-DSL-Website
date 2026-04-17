# NeuroChain Stellar - Stellar Actions Guide

This file is the technical reference for the `neurochain-stellar` CLI path.

Keep this document in sync with implementation details, edge cases, examples, and parity notes.

## What This Is

`neurochain-stellar` reads `.nc` files and converts action lines into **ActionPlan** JSON. When `--flow` is enabled, it runs:

**simulate -> preview -> confirm -> submit**

Currently supported actions:

- **FundTestnet** via Friendbot
- **BalanceQuery** via Horizon
- **CreateAccount** via Stellar CLI
- **ChangeTrust** via Stellar CLI
- **Payment** via Stellar CLI, including XLM and issued assets
- **TxStatus** via Horizon
- **Soroban deploy** via Stellar CLI
- **Soroban invoke** via Stellar CLI

---

## 1) Installation And Basic Setup

### Required Tools

- Rust + Cargo
- `stellar` CLI for Stellar commands

### Cargo Argument Syntax

Important rule: `cargo run` needs `--` before arguments that should go to the **neurochain-stellar** binary instead of Cargo.

```powershell
# CORRECT: REPL, flow enabled by default
cargo run --release --bin neurochain-stellar

# CORRECT: plan-only REPL, no simulate/submit
cargo run --release --bin neurochain-stellar -- --no-flow

# CORRECT: explicit flow, optional for REPL
cargo run --release --bin neurochain-stellar -- --flow

# WRONG: --flow goes to Cargo and fails with "unexpected argument '--flow'"
cargo run --release --bin neurochain-stellar --flow
```

Notes:

- `cargo run --bin neurochain-stellar ...` without `--release` uses **DEBUG/DEV mode** (`target\debug\...`).
- `cargo run --release --bin neurochain-stellar ...` uses **RELEASE mode** (`target\release\...`) and is optimized for runtime.

### Main Commands, Recommended Release Mode

```powershell
cd <project-root>

# 1) Normal CLI/REPL run
cargo run --release --bin neurochain-stellar

# 2) Plan-only REPL, if you do not want simulate/submit in this session
cargo run --release --bin neurochain-stellar -- --no-flow
```

These two commands are the main daily workflow.

Normal REPL runs the flow path by default:

`simulate -> preview -> confirm -> submit`

### Debug Commands

```powershell
# Normal CLI/REPL run in debug mode
cargo run --bin neurochain-stellar

# Plan-only REPL in debug mode
cargo run --bin neurochain-stellar -- --no-flow
```

### Other Run Modes, Release

```powershell
# Intent prompt directly from a CLI flag
cargo run --release --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..."

# Same command with intent debug trace
cargo run --release --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..." --debug

# .nc file
cargo run --release --bin neurochain-stellar -- examples\intent_stellar_payment_flow.nc

# .nc flow: simulate -> preview -> confirm -> submit
cargo run --release --bin neurochain-stellar -- examples\intent_stellar_payment_flow.nc --flow
```

For debug mode, use the same commands without `--release`.

---

## 2) Environment Variables

### Network And API

- `NC_STELLAR_NETWORK` / `NC_SOROBAN_NETWORK`
  - Default: `testnet`
- `NC_STELLAR_HORIZON_URL`
  - Default: derived from the active network
- `NC_FRIENDBOT_URL`
  - Testnet only
  - Default: Stellar testnet Friendbot

### Soroban Invoke

- `NC_SOROBAN_SOURCE` or `NC_STELLAR_SOURCE`
  - Stellar CLI key alias
  - Do not put secret keys directly in files or docs
- `NC_STELLAR_CLI`
  - Use this if `stellar` is not in `PATH`
- `NC_SOROBAN_SIMULATE_FLAG`
  - Default: `--send no` for CLI 25+
  - Examples: `--send no` or `--send=no`
- `NC_TXREP_PREVIEW=1`
  - Adds txrep/SEP-11 preview output for human-readable XDR
  - If the CLI does not support `tx to-rep`, the fallback is `tx decode` with `json-formatted` output

### IntentStellar Mode

- `NC_INTENT_STELLAR_MODEL`
  - IntentStellar ONNX model path
  - Default: `models/intent_stellar/model.onnx`
- `NC_INTENT_STELLAR_THRESHOLD`
  - Confidence threshold
  - Default: `0.55`
- `NC_INTENT_DEBUG=1`
  - Enables intent pipeline trace:
  - `classify -> slot-parse -> guardrails -> flow`

### x402-lite

- `NC_X402=1`
  - Enables x402-lite commands in REPL and `.nc` scripts

### Allowlist, Optional But Recommended

- `NC_ASSET_ALLOWLIST`
  - Example: `XLM,USDC:GISSUER`
- `NC_SOROBAN_ALLOWLIST`
  - Example: `C1:transfer,C2`
- `NC_ALLOWLIST_ENFORCE=1`
  - Hard-fails allowlist violations
  - Without enforce mode, violations are warnings

### Contract Policy, Optional But Recommended

- `NC_CONTRACT_POLICY`
  - Direct path to `policy.json`
- `NC_CONTRACT_POLICY_DIR`
  - Policy directory
  - Default: `contracts`
- `NC_CONTRACT_POLICY_ENFORCE=1`
  - Hard-fails policy violations
  - Without enforce mode, violations are warnings

### 2.1) Environment Matrix

The same environment variables apply to CLI runs (`--intent-text` / file mode) and `.nc` script runs.

In REPL, the same values can also be set with commands such as `network: ...`, `wallet: ...`, and `txrep`.

| Env | Purpose | Applies To | Default |
|---|---|---|---|
| `NC_STELLAR_NETWORK` / `NC_SOROBAN_NETWORK` | Sets the network | CLI + REPL + `.nc` | `testnet` |
| `NC_STELLAR_HORIZON_URL` | Sets the Horizon URL | CLI + REPL + `.nc` | derived from network |
| `NC_FRIENDBOT_URL` | Sets the Friendbot URL | CLI + REPL + `.nc` | testnet friendbot |
| `NC_SOROBAN_SOURCE` / `NC_STELLAR_SOURCE` | Sets the source wallet alias | CLI + `.nc`; REPL wallet is set explicitly | not set |
| `NC_STELLAR_CLI` | Sets the `stellar` binary | CLI + REPL + `.nc` | `stellar` |
| `NC_SOROBAN_SIMULATE_FLAG` | Sets the simulate flag | CLI + REPL + `.nc` | `--send no` |
| `NC_TXREP_PREVIEW` | Enables txrep preview | CLI + REPL + `.nc` | off |
| `NC_X402` | Enables x402-lite commands | CLI + REPL + `.nc` | off |
| `NC_INTENT_STELLAR_MODEL` | Sets the IntentStellar model path | CLI + REPL + `.nc` | `models/intent_stellar/model.onnx` |
| `NC_INTENT_STELLAR_THRESHOLD` | Sets the intent confidence threshold | CLI + REPL + `.nc` | `0.55` |
| `NC_INTENT_DEBUG` | Enables intent debug trace | CLI + REPL + `.nc` | off |
| `NC_ASSET_ALLOWLIST` | Sets the asset allowlist | CLI + REPL + `.nc` | empty |
| `NC_SOROBAN_ALLOWLIST` | Sets the contract/function allowlist | CLI + REPL + `.nc` | empty |
| `NC_ALLOWLIST_ENFORCE` | Hard-fails allowlist violations | CLI + REPL + `.nc` | off, warning-only |
| `NC_CONTRACT_POLICY` | Sets one policy JSON path | CLI + REPL + `.nc` | not set |
| `NC_CONTRACT_POLICY_DIR` | Sets the policy directory | CLI + REPL + `.nc` | `contracts` |
| `NC_CONTRACT_POLICY_ENFORCE` | Hard-fails policy violations | CLI + REPL + `.nc` | off, warning-only |

### 2.2) Same Setup Without Environment Variables

You can set the same values directly inside the CLI REPL or inside a `.nc` script.

Recommended order:

`AI -> network -> wallet -> other settings -> intent/action`

- `NC_STELLAR_NETWORK` / `NC_SOROBAN_NETWORK` -> `network: testnet`
- `NC_STELLAR_HORIZON_URL` -> `horizon: https://horizon-testnet.stellar.org`
- `NC_FRIENDBOT_URL` -> `friendbot: https://friendbot.stellar.org` or `friendbot: off`
- `NC_SOROBAN_SOURCE` / `NC_STELLAR_SOURCE` -> `wallet: nc-testnet` or `source: nc-testnet`
- Key alias creation for development -> `wallet_generate: demo-alias`
- One-line testnet wallet bootstrap -> `wallet_bootstrap: demo-alias`
- `NC_STELLAR_CLI` -> `stellar_cli: stellar`
- `NC_SOROBAN_SIMULATE_FLAG` -> `simulate_flag: "--send no"`
- `NC_TXREP_PREVIEW=1` -> `txrep` / `txrep on` / `txrep off`
- `NC_X402=1` -> `x402` / `x402 on` / `x402 off`
- `NC_INTENT_STELLAR_MODEL` -> `AI: "models/intent_stellar/model.onnx"`
- `NC_INTENT_STELLAR_THRESHOLD` -> `intent_threshold: 0.55`
- `NC_INTENT_DEBUG=1` -> `debug` / `debug off`
- `NC_ASSET_ALLOWLIST` -> `asset_allowlist: XLM,USDC:GISSUER`
- `NC_SOROBAN_ALLOWLIST` -> `soroban_allowlist: C1:transfer,C2`
- `NC_ALLOWLIST_ENFORCE` -> `allowlist_enforce` / `allowlist_enforce off`
- `NC_CONTRACT_POLICY` -> `contract_policy: contracts/<id>/policy.json`
- `NC_CONTRACT_POLICY_DIR` -> `contract_policy_dir: contracts`
- `NC_CONTRACT_POLICY_ENFORCE` -> `contract_policy_enforce` / `contract_policy_enforce off`

Example for REPL or `.nc`:

```nc
AI: "models/intent_stellar/model.onnx"
network: testnet
wallet: nc-testnet
txrep
asset_allowlist: XLM
allowlist_enforce
contract_policy: contracts/CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ/policy.json
contract_policy_enforce
set stellar intent from AI: "Transfer 5 XLM to G..."
```

Testnet USDC example from Stellar Expert:

```powershell
setx NC_ASSET_ALLOWLIST "XLM,USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
```

### 2.5) Enforce Behavior And Exit Codes

Validation always runs. Enforce mode controls whether a violation becomes a hard block.

- `NC_ALLOWLIST_ENFORCE=0`, or unset:
  - allowlist violations are warnings and execution continues
- `NC_ALLOWLIST_ENFORCE=1`:
  - allowlist violation hard-fails with exit code `3`
- `NC_CONTRACT_POLICY_ENFORCE=0`, or unset:
  - policy violations are warnings and execution continues
- `NC_CONTRACT_POLICY_ENFORCE=1`:
  - policy violation hard-fails with exit code `4`
- In intent mode (`--intent-text` or `set stellar intent from AI`):
  - `Unknown`, `intent_error`, and `intent_warning` block flow safely and return exit code `5`

Typed template v2, policy-backed:

- If `contract_policy.args_schema` defines a typed argument (`address` / `bytes` / `symbol` / `u64`) and the prompt provides an invalid value, intent mode converts the result into:
  - `slot_type_error -> Unknown -> safe no-submit`
  - exit code `5`
- This is consistent across CLI, REPL, `.nc`, and `/api/stellar/intent-plan`.
- A missing required argument remains a policy error:
  - `policy_args_missing`
  - with enforce mode, it blocks with exit code `4`

---

## 3) Usage: JSON ActionPlan Only

```powershell
cargo run --bin neurochain-stellar -- examples\stellar_actions_example.nc
```

The output is ActionPlan JSON describing what NeuroChain would do.

## 3.5) Usage: `--intent-text`

IntentStellar converts natural language into an ActionPlan.

```powershell
cargo run --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..."
```

Deploy phase 1, rule-based fallback without model retraining:

```powershell
cargo run --bin neurochain-stellar -- --intent-text "Deploy contract alias hello-demo wasm ./contracts/hello.wasm"
```

Override model path and threshold:

```powershell
cargo run --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..." --intent-model models\intent_stellar\model.onnx --intent-threshold 0.60
```

Safety block behavior:

- If confidence is low or required slots are missing, the ActionPlan contains `unknown` plus `intent_error` or `intent_warning`.
- In `--flow` mode, submit is skipped safely and the process returns exit code `5`.

## 3.6) Usage: Interactive REPL

```powershell
cargo run --bin neurochain-stellar
```

Wallet startup behavior:

- REPL always starts with `Current wallet/source: (not set)`.
- This is intentional wallet-explicit UX.
- Set the active wallet yourself with `wallet: <alias>` or `source: <alias>`.
- `setup testnet` does not set the wallet automatically.
- Default REPL `asset_allowlist` is `XLM`, unless overridden by environment or command.

REPL commands from `help all`:

Core setup, value required:

- `AI: "path"` -> set intent model path
- `intent_threshold: <f32>` -> set intent confidence threshold
- `network: testnet|mainnet|public` -> set active network for flow
- `wallet: <stellar-key-alias>` -> set active source wallet alias
- `wallet_generate: <alias>` -> generate a local Stellar key alias
- `wallet_bootstrap: <alias>` -> generate an alias and fund it with Friendbot
- `horizon: https://...` -> set Horizon URL override
- `friendbot: https://...|off` -> set Friendbot URL or disable it
- `stellar_cli: <bin>` -> set Stellar CLI binary path/name
- `simulate_flag: "--send no"` -> set Soroban simulate flag
- `asset_allowlist: XLM,USDC:G...` -> set the equivalent of `NC_ASSET_ALLOWLIST`
- `soroban_allowlist: C1:transfer,C2` -> set the equivalent of `NC_SOROBAN_ALLOWLIST`
- `contract_policy: <path>` -> set the equivalent of `NC_CONTRACT_POLICY`
- `contract_policy_dir: <dir>` -> set the equivalent of `NC_CONTRACT_POLICY_DIR`

Toggles:

- `txrep` -> enable txrep preview in flow
- `txrep off` -> disable txrep preview in flow
- `x402` -> enable x402-lite flow commands
- `x402 off` -> disable x402-lite flow commands
- `allowlist_enforce` -> enable allowlist enforcement
- `allowlist_enforce off` -> disable allowlist enforcement
- `contract_policy_enforce` -> enable contract policy enforcement
- `contract_policy_enforce off` -> disable contract policy enforcement
- `debug` -> enable intent pipeline trace
- `debug off` -> disable intent pipeline trace

Prompt and action commands:

- `set <var> from AI: "..."` -> predict with the active model and store the result
- `set stellar intent from AI: "..."` -> classify prompt into an ActionPlan
- `set intent from AI: "Transfer 5 XLM to G..."` -> legacy alias, still supported
- `macro from AI: "..."` -> not supported in `neurochain-stellar`; use `set stellar intent from AI`
- `plain text prompt` -> classify prompt into an ActionPlan
- `stellar.* / soroban.* lines` -> manual action-plan mode
- `soroban.contract.deploy alias="..." wasm="..."` -> manual deploy action
- `x402.request to="G..." amount="1" asset_code="XLM"` -> create an x402-lite payment challenge
- `x402.finalize challenge_id="last"` -> finalize a challenge into a typed `stellar_payment` action

Utility commands:

- `help` -> quick start
- `help all` -> show every command
- `help dsl` -> show normal NeuroChain DSL language help
- `show setup` -> print active setup
- `show config` -> print active config
- `setup testnet` -> set network, Horizon, and Friendbot baseline
- `exit` -> leave REPL

Unified toggle rule:

- A bare setting line enables the toggle:
  - `txrep`, `x402`, `allowlist_enforce`, `contract_policy_enforce`, `debug`
- Adding `off` disables the toggle:
  - `txrep off`, `x402 off`, `allowlist_enforce off`, `contract_policy_enforce off`, `debug off`

## 3.7) Usage: `.nc` Scripts With The Same Commands

The same meta lines also work in files:

```nc
AI: "models/intent_stellar/model.onnx"
network: testnet
wallet: nc-testnet
txrep
set stellar intent from AI: "Transfer 5 XLM to G..."
```

```powershell
cargo run --bin neurochain-stellar -- examples\intent_stellar_smoke.nc --flow
```

Notes:

- Script mode prints a `Script execution setup` summary to stderr before execution.
- The summary shows active settings such as `network`, `wallet/source`, `flow_mode`, `txrep_preview`, and allowlists.
- `.nc` script mode follows the same rules as CLI and REPL:
  - same `validate_plan` validation
  - same allowlist and policy enforcement behavior
  - same `--flow` versus plan-only behavior
  - same intent safety block rules and exit codes

### 3.7.1) Multi-Model `if` Pipeline In A Single `.nc` Run

A script can use multiple models in one run:

```nc
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "This is wonderful!"
if mood == "Positive":
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to G..."
```

Ready-made example:

- `examples/multi_model_if_payment.nc`

Golden path, model-agnostic gate:

- `examples/golden_path_model_agnostic.nc`
- `examples/golden_path_model_agnostic_blocked.nc`
- One unified structure:
  - `set <var> from AI`
  - `if`
  - `set stellar intent from AI`
- You only change the gate model, prompt, and `allow_label` value for SST2/factcheck/toxic/etc.

```powershell
cargo run --release --bin neurochain-stellar -- examples\golden_path_model_agnostic.nc --flow
cargo run --release --bin neurochain-stellar -- examples\golden_path_model_agnostic_blocked.nc --flow
```

## 3.8) `--flow` Versus Plan-Only

- In REPL (`cargo run --bin neurochain-stellar`), flow is enabled by default.
- `--no-flow` forces REPL plan-only mode with no simulate/submit.
- File and `--intent-text` runs without `--flow` print only ActionPlan JSON.
- File and `--intent-text` runs with `--flow` run:
  - `simulate -> preview -> confirm -> submit`
- `Y/N` confirmation appears only in flow mode:
  - `Confirm submit? [y/N]`
- `--yes` skips the confirmation prompt in flow mode.
- `NC_TXREP_PREVIEW=1` affects the preview phase, so it is visible in flow runs.

Quick summary:

- `cargo run --bin neurochain-stellar` = REPL, flow enabled by default
- `cargo run --bin neurochain-stellar -- --no-flow` = REPL plan-only
- `cargo run --bin neurochain-stellar -- <input>` = file/intent dry-run
- `cargo run --bin neurochain-stellar -- <input> --flow` = file/intent run that can submit

---

## 4) Usage: Simulate, Preview, Confirm, Submit

```powershell
cargo run --bin neurochain-stellar -- examples\stellar_actions_example.nc --flow
```

- Preview shows estimated fee from Horizon `fee_stats` and action effects.
- `--yes` skips the confirmation prompt.
- Submit output shows a **tx hash** when it can be derived.
- If a hash is not available in CLI output, the latest tx hash is fetched from Horizon and marked with `(latest)`.
- Submit rows use a unified format:
  - `status=ok|error`, `tx_hash`, `return`
- Empty Soroban simulation output is interpreted as an `ok` result.
- If `NC_TXREP_PREVIEW=1`, preview prints txrep for each action.
- If `to-rep` is not available, `tx decode` JSON is printed instead.

## 4.5) Contract Policy

Soroban invoke can be validated with a contract-specific policy before the simulate path.

- Policy file:
  - `contracts/<name>/policy.json`
  - or direct path via `NC_CONTRACT_POLICY`
- Enforce:
  - `NC_CONTRACT_POLICY_ENFORCE=1` hard-fails violations

Supported argument types:

- `string`
- `number`
- `bool`
- `address`
  - strkey, `G...` or `C...`, 56 chars
- `symbol`
  - 1 to 32 ASCII chars, no whitespace
- `bytes`
  - hex format, `0x...`
- `u64`
  - non-negative integer as JSON number or string, such as `100`

You can force typed-slot validation in an IntentStellar `ContractInvoke` prompt:

```text
Invoke contract C... function transfer args={"to":"G...","amount":100} arg_types={"to":"address","amount":"u64"}
```

If a type does not match, the result is:

`slot_type_error -> Unknown -> safe no-submit`

Flow is blocked.

Policy can also run typed-v2 checks without `arg_types=`:

- If `args_schema` defines `hello.to = symbol` and the prompt provides `args={"to":"Hello World"}`, the value is rejected in intent mode as `slot_type_error`.
- The resulting plan is `Unknown`, safe no-submit, exit `5`.
- This only applies to wrong types.
- A missing required field remains a policy-layer `policy_args_missing` error.

Example `hello` contract policy:

```json
{
  "contract_id": "C...",
  "allowed_functions": ["hello"],
  "args_schema": {
    "hello": {
      "required": { "to": "symbol" },
      "optional": {}
    }
  },
  "max_fee_stroops": 1000
}
```

```powershell
cargo run --bin neurochain-stellar -- examples\stellar_actions_example.nc --flow --yes
```

Policy typed v2 fail/pass examples:

- `examples/intent_stellar_policy_typed_slot_error.nc`
  - policy-backed type mismatch
  - `slot_type_error`, safe no-submit, exit `5`
- `examples/intent_stellar_policy_typed_slot_ok.nc`
  - policy-backed type OK
  - action remains `soroban_contract_invoke`
- `examples/intent_stellar_policy_typed_stage2_normalize.nc`
  - stage 2 normalization
  - `" World "` -> `"World"`
  - action remains `soroban_contract_invoke`
- `examples/intent_stellar_typed_template_stage3_ok.nc`
  - template-side `arg_types=` normalization for address/bytes/symbol/u64
- `examples/intent_stellar_typed_template_stage3_error.nc`
  - template-side `arg_types=` multi-error
  - `slot_type_error`, flow block, exit `5`

Minimal REPL commands:

```text
AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
contract_policy: contracts/CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ/policy.json
contract_policy_enforce

# FAIL -> slot_type_error -> unknown -> exit 5 in flow
set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"Hello World\"}"

# PASS -> soroban_contract_invoke
set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"World\"}"
```

Minimal `.nc` + CLI commands:

```powershell
cargo run --release --bin neurochain-stellar -- examples\intent_stellar_policy_typed_slot_error.nc --flow --yes
cargo run --release --bin neurochain-stellar -- examples\intent_stellar_policy_typed_slot_ok.nc --flow --yes
```

Minimal env-var commands:

```powershell
$env:NC_CONTRACT_POLICY="contracts\CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ\policy.json"
$env:NC_CONTRACT_POLICY_ENFORCE="1"
cargo run --release --bin neurochain-stellar -- --intent-text "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"Hello World\"}" --flow --yes
```

REPL quick-start, policy + typed v2 fail/pass:

```text
AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
contract_policy: contracts/CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ/policy.json
contract_policy_enforce

# FAIL: policy requires hello.to = symbol, and "Hello World" is not a valid symbol
set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"Hello World\"}"

# PASS: valid symbol
set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={\"to\":\"World\"}"
```

What happens:

- FAIL line:
  - `slot_type_error -> unknown`
  - flow is blocked safely
  - exit `5` or REPL step code `5`
- PASS line:
  - ActionPlan keeps `soroban_contract_invoke`
- If the field is completely missing, such as missing `to`, the policy layer returns `policy_args_missing`.
- With enforce mode, missing required args block with exit `4`.

Typed templates v2, stage 2 normalization and error reporting:

- `symbol`
  - trims edge whitespace before validation
  - example: `" World "` -> `"World"`
- `bytes`
  - normalizes common hex forms
  - examples: `0X0A0B` or `0A0B` -> `0x0a0b`
- `u64`
  - accepts strings
  - example: `"00100"` -> JSON number `100`
- `address`
  - trims and normalizes to uppercase before validation
- If multiple typed args are invalid in the same request, you get multiple `slot_type_error` warnings, one per argument.

Test coverage:

- `tests/stellar_repl.rs`
  - REPL `contract_policy` / `contract_policy_enforce` settings without env vars
  - typed mismatch visibility
- `tests/stellar_script.rs`
  - `.nc` script policy settings without env vars
- `tests/flow_cli.rs`
  - `--flow` blocks on `slot_type_error` with exit `5`
- `tests/server_analyze.rs`
  - `/api/stellar/intent-plan` returns policy-derived `slot_type_error` as a block with `exit_code=5`
- `src/intent_stellar.rs`
  - typed slot normalization and multi-error reporting for the `arg_types=` path
- `tests/flow_cli.rs`
  - policy-backed typed v2 edge tests for `address`, `bytes`, `symbol`, `u64`

---

## 5) `.nc` Lines

Manual action lines start with `stellar.` or `soroban.`.

Inline comments with `#` and `//` are allowed.

Comment-only lines, starting with `#` or `//`, are skipped entirely.

```nc
# BalanceQuery
stellar.account.balance account="G..." asset="XLM"

# Fund testnet
stellar.account.fund_testnet account="G..."

# Soroban invoke
soroban.contract.invoke contract_id="C..." function="transfer" args={"to":"G...","amount":100}

# Soroban deploy, manual
soroban.contract.deploy alias="hello-demo" wasm="./contracts/hello.wasm"
```

Amounts such as `amount`, `starting_balance`, and `limit` are interpreted as XLM-style decimal values and converted to stroops with 7 decimal places before submit.

---

## 6) Issued Asset Flow

The repo includes a ready-made **TESTUSD** flow using a test issuer-owned asset:

```text
examples/stellar_testasset_trustline.nc
examples/stellar_testasset_issue.nc
examples/stellar_testasset_payment.nc
examples/stellar_testasset_user_trustline.nc
examples/stellar_testasset_user_payment.nc
```

The basic issued-asset flow has two phases:

1. Receiver creates a trustline.
2. Sender sends the issued asset payment.

For the receiver step, set `NC_SOROBAN_SOURCE=<receiver-alias>` and keep only the `change_trust` line active.

For the sender step, set `NC_SOROBAN_SOURCE=<sender-alias>` and keep only the payment line active.

Use the tracked TESTUSD examples instead of non-public USDC placeholders:

- `examples/stellar_testasset_trustline.nc` -> receiver trustline
- `examples/stellar_testasset_issue.nc` -> issuer funds the distribution account
- `examples/stellar_testasset_payment.nc` -> distribution account sends TESTUSD
- `examples/stellar_testasset_user_trustline.nc` -> end-user trustline
- `examples/stellar_testasset_user_payment.nc` -> end-user TESTUSD payment

Usage commands:

```powershell
# Receiver trustline
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_trustline.nc --flow

# Sender TESTUSD payment
$env:NC_SOROBAN_SOURCE="nc-testnet"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_payment.nc --flow
```

Test-asset flow with your own issuer:

```powershell
# 1) Receiver trustline (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_trustline.nc --flow

# 2) Issuer sends TESTUSD to receiver (nc-testnet)
$env:NC_SOROBAN_SOURCE="nc-testnet"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_issue.nc --flow

# 3) Receiver sends TESTUSD back (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_payment.nc --flow
```

Three-account model, distributor to user:

Replace `GUSER...` with the actual user account and run:

```powershell
# User trustline (user alias)
$env:NC_SOROBAN_SOURCE="user-alias"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_user_trustline.nc --flow

# Distributor -> user payment (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_user_payment.nc --flow
```

---

## 7) Soroban Invoke And Deploy Require A CLI Key

Soroban invoke and deploy use `stellar contract ...` commands. Set a key alias:

```powershell
# Example: set key alias "quest1-new" and use it
setx NC_SOROBAN_SOURCE "quest1-new"
```

---

## 8) Txrep Conversions

NeuroChain includes two txrep conversion tools:

- `txrep-to-action`
  - converts `stellar tx decode --output json-formatted` data into **ActionPlan**
- `txrep-to-jsonl`
  - converts the same txrep data into JSONL rows for dataset pipelines

Example:

```powershell
# 1) Decode XDR -> txrep json-formatted
stellar tx decode --input <TX_XDR_BASE64> --output json-formatted > txrep.json

# 2) Txrep -> ActionPlan
cargo run --bin txrep-to-action -- txrep.json > action_plan.json

# 3) Txrep -> JSONL dataset rows
cargo run --bin txrep-to-jsonl -- txrep.json > dataset.jsonl
```

These tools do not make on-chain calls. They only convert data.

---

## 9) Common Errors

- **Friendbot error**
  - verify testnet and public key
- **Horizon 404**
  - the account may not exist or may not be funded yet
- **Soroban invoke failed**
  - check `contract_id`, function name, allowlist, and CLI key alias

---

## 10) Roadmap

- More detailed Soroban invoke output parsing for fee and preview output.
- Optional txrep / SEP-11 preview output for a human-readable XDR audit trail.

---

## 11) Update Rule

Update this guide whenever:

- new actions are added
- preview output expands
- guardrail behavior changes

---

## 12) Server API: IntentStellar To ActionPlan

Start the server:

```powershell
cd C:\Users\Ville\Desktop\neurochain_dsl_stellar
cargo run --release --bin neurochain-server
```

The same binary serves both:

- `/api/analyze`
- `/api/stellar/intent-plan`

Optional environment variables:

```powershell
$env:HOST="127.0.0.1"
$env:PORT="8081"
$env:NC_MODELS_DIR="models"
$env:NC_API_KEY="your-secret-key"
cargo run --release --bin neurochain-server
```

Endpoint:

```http
POST /api/stellar/intent-plan
```

Example:

```powershell
$body = @{
  model     = "intent_stellar"
  prompt    = "Check balance for GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX asset XLM"
  threshold = 0.55
} | ConvertTo-Json

Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:8081/api/stellar/intent-plan" -ContentType "application/json" -Body $body
```

Response contains:

- `plan`
  - ActionPlan JSON
- `blocked`
  - whether the request is blocked
- `exit_code`
  - same block codes:
  - `3` allowlist
  - `4` policy
  - `5` intent safety
- `logs`
  - diagnostic messages

The endpoint uses the same intent core as the CLI:

- `classify_intent_stellar`
- `build_intent_action_plan`

Guardrail behavior is therefore consistent across REPL, `.nc`, and server paths.
