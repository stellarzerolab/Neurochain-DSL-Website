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
| `NC_ZK_GUARDRAIL_CONTRACT` | Sets the deployed NeuroChain ZK Guardrail contract ID | REPL ZK bridge | not set |
| `NC_ZK_INSTRUCTION_LEEWAY` | Sets the ZK verifier instruction leeway | REPL ZK bridge | `10000000` |
| `NC_STELLAR_SCRIPT_UNSAFE_EXEC` | Allows trusted `.nc` scripts to run local setup side effects during script build | `.nc` only | off |
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
- `NC_ZK_GUARDRAIL_CONTRACT` -> `zk_contract: C...`
- `NC_ZK_INSTRUCTION_LEEWAY` -> `zk_instruction_leeway: 10000000`

Safety notes:

- In the REPL, `wallet_generate`, `wallet_bootstrap`, and `stellar_cli` are
  interactive local commands.
- In `.nc` script build / plan-only mode, those same directives are blocked by
  default because they can execute local processes before the final submit
  confirmation. Use `NC_STELLAR_SCRIPT_UNSAFE_EXEC=1` only for trusted local
  scripts.
- `simulate_flag` can still be shown and configured for compatibility, but
  Soroban preview simulation always enforces `--send no` before any
  `Confirm submit?` boundary.

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
  - if the relevant asset or contract allowlist is empty, execution also
    hard-fails with exit code `3`
- `NC_CONTRACT_POLICY_ENFORCE=0`, or unset:
  - policy violations are warnings and execution continues
- `NC_CONTRACT_POLICY_ENFORCE=1`:
  - policy violation hard-fails with exit code `4`
  - missing, unreadable, or invalid policy files hard-fail with exit code `4`
  - contract actions hard-fail with exit code `4` if no contract policies are
    loaded
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

Soroban deep templates v2, policy-backed:

- `policy.json` can define `intent_templates` for a contract.
- A template maps high-level wording such as `say hello to World` into a deterministic `soroban.contract.invoke` plan.
- This removes the need for the prompt to always contain `contract_id`, `function`, `args`, and `arg_types`.
- The template still runs through the same allowlist, policy, typed-slot, simulate/preview, and flow rules.
- Low-confidence intent warnings are not bypassed.

Minimal policy shape:

```json
{
  "contract_id": "C...",
  "allowed_functions": ["hello"],
  "args_schema": {
    "hello": {
      "required": {
        "to": "symbol"
      },
      "optional": {}
    }
  },
  "intent_templates": {
    "hello": {
      "aliases": ["say hello", "hello contract", "greet"],
      "function": "hello",
      "args": {
        "to": {
          "source": "after_to",
          "type": "symbol",
          "default": "World"
        }
      }
    }
  }
}
```

Template arg sources currently supported:

- `after_to`
- `after_for`
- `quoted` / `first_quoted`
- `first_account`
- `first_contract`
- `first_number`

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
- `wallet_generate: <alias>` -> generate a local Stellar key alias (REPL; `.nc` requires `NC_STELLAR_SCRIPT_UNSAFE_EXEC=1`)
- `wallet_bootstrap: <alias>` -> generate an alias and fund it with Friendbot (REPL; `.nc` requires `NC_STELLAR_SCRIPT_UNSAFE_EXEC=1`)
- `horizon: https://...` -> set Horizon URL override
- `friendbot: https://...|off` -> set Friendbot URL or disable it
- `stellar_cli: <bin>` -> set Stellar CLI binary path/name (REPL; `.nc` requires `NC_STELLAR_SCRIPT_UNSAFE_EXEC=1`)
- `simulate_flag: "--send no"` -> set Soroban simulate flag; preview still enforces `--send no`
- `zk_contract: C...` -> set the deployed NeuroChain ZK Guardrail contract
- `zk_instruction_leeway: 10000000` -> set verifier instruction leeway
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

ZK Guardrail commands:

- `zk.demo approved|requires_approval|blocked` -> validate a bundled public binding locally
- `zk.verify action_plan="..." proof="..."` -> validate caller-selected public files locally
- `zk.stellar.verify approved|requires_approval|blocked|last` -> cryptographically verify on Soroban without changing state
- `zk.stellar.attest approved|requires_approval|blocked|last` -> submit a real testnet proof-verification transaction and print its explorer link
- `zk.stellar.consume approved|requires_approval|blocked|last` -> owner-only nullifier consume in local flow mode
- `zk status` -> show the local binding together with the last Stellar result and transaction hash

Soroban v2 templates:

- `template registry` -> policy-backed `intent_templates` in the contract policy
- `hello` -> prompt example: `Please say hello to World`
- `claim_rewards` -> prompt example: `Invoke contract rewards function claim_rewards`
- `deposit` -> prompt example: `Invoke contract deposit amount 100 asset USDC`
- `swap` -> prompt shape: amount / from asset / to asset / `min_out`
- `parity` -> same template core in REPL, `.nc`, and `/api/stellar/intent-plan`

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
  "intent_templates": {
    "hello": {
      "aliases": ["say hello", "hello contract", "greet"],
      "function": "hello",
      "args": {
        "to": {
          "source": "after_to",
          "type": "symbol",
          "default": "World"
        }
      }
    }
  },
  "max_fee_stroops": 1000
}
```

```powershell
cargo run --bin neurochain-stellar -- examples\stellar_actions_example.nc --flow --yes

$env:NC_CONTRACT_POLICY="contracts\CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ\policy.json"
cargo run --bin neurochain-stellar -- --intent-text "Please say hello to World" --intent-threshold 0.00
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
- server endpoints or response shapes change

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
- `/api/x402/stellar/intent-plan`

Optional environment variables:

```powershell
$env:HOST="127.0.0.1"
$env:PORT="8081"
$env:NC_MODELS_DIR="models"
$env:NC_API_KEY="your-secret-key"
$env:NC_X402_STELLAR_AUDIT_PATH="logs/x402_stellar_audit.jsonl"
$env:NC_X402_STELLAR_STORE_PATH="logs/x402_stellar_store.json"
cargo run --release --bin neurochain-server
```

The server API accepts model ids such as `intent_stellar`, not arbitrary
client-provided model paths. Requests that include `model_path` are rejected
before loading any ONNX/tokenizer files.

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

### x402-paid Stellar IntentPlan Gateway

Endpoint:

```http
POST /api/x402/stellar/intent-plan
```

This endpoint is an x402-lite access layer in front of the same IntentStellar
planning engine. It does not submit transactions and it does not bypass
guardrails.

Facilitator boundary:

- the current verifier is still the local mock verifier
- the public response envelope is already facilitator-shaped
- server route code talks to a payment verifier adapter instead of hard-coding
  the mock finalize logic directly in the route
- future real x402 `verify` / `settle` / facilitator logic should replace the
  verifier implementation, not the frontend/agent response contract

What stays stable for agents and frontends:

- `audit_id`
- `payment`
- `decision`
- `guardrails`
- `logs`
- finalized responses also include `plan`

What is still mock-only today:

- the payment proof format is `PAYMENT-SIGNATURE: paid:<challenge_id>`
- challenge finalization is local store-backed, not a real facilitator proof
- pricing, receiver account, auth boundary, and production settlement are still
  later decisions

Unpaid request:

```powershell
$body = @{
  model     = "intent_stellar"
  prompt    = "Check balance for GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX asset XLM"
  threshold = 0.0
} | ConvertTo-Json

Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:8081/api/x402/stellar/intent-plan" -ContentType "application/json" -Body $body
```

Expected unpaid behavior:

- HTTP `402 Payment Required`
- `payment.state = "payment_required"`
- `decision.status = "not_evaluated"`
- `guardrails.state = "not_run"`
- response includes `challenge_id`, `expires_at`, amount/asset/network/receiver and a mock `PAYMENT-SIGNATURE` hint

Mock finalized request:

```powershell
$challengeId = "<challenge_id from the 402 response>"

Invoke-RestMethod `
  -Method Post `
  -Uri "http://127.0.0.1:8081/api/x402/stellar/intent-plan" `
  -ContentType "application/json" `
  -Headers @{ "PAYMENT-SIGNATURE" = "paid:$challengeId" } `
  -Body $body
```

Finalized response contains both the old compatibility fields and the newer
decision model:

- top-level compatibility:
  - `ok`
  - `blocked`
  - `exit_code`
  - `error`
  - `plan`
  - `logs`
- `audit_id`
- `payment`
  - `protocol = "x402"`
  - `state = "finalized"` / `payment_required` / `replay_blocked` / `expired` / `invalid`
  - `challenge_id`
  - amount/asset/network/receiver
  - `created_at`, `expires_at`, `finalized_at`
- `decision`
  - `status = "approved" | "requires_approval" | "blocked" | "not_evaluated"`
  - `approved`
  - `blocked`
  - `requires_approval`
  - `reason`
- `guardrails`
  - `state = "passed" | "blocked" | "not_run"`
  - `exit_code`
  - `reason = "allowlist" | "contract_policy" | "intent_safety" | ...`

Agent/frontend response contract:

| Scenario | HTTP | `payment.state` | `decision.status` | `decision.reason` | `guardrails.state` | `guardrails.exit_code` |
| --- | --- | --- | --- | --- | --- | --- |
| payment required | `402` | `payment_required` | `not_evaluated` | `null` | `not_run` | `null` |
| approved | `200` | `finalized` | `approved` | `null` | `passed` | `null` |
| requires approval | `200` | `finalized` | `requires_approval` | `approval_required` | `passed` | `null` |
| allowlist block | `200` | `finalized` | `blocked` | `allowlist` | `blocked` | `3` |
| contract policy block | `200` | `finalized` | `blocked` | `contract_policy` | `blocked` | `4` |
| intent safety / slot block | `200` | `finalized` | `blocked` | `intent_safety` | `blocked` | `5` |
| replay block | `409` | `replay_blocked` | `blocked` | `payment_replay_blocked` | `not_run` | `null` |
| expired challenge | `402` | `expired` | `blocked` | `payment_expired` | `not_run` | `null` |
| invalid payment proof | `402` | `invalid` | `blocked` | `invalid_payment` | `not_run` | `null` |

For every scenario, clients can rely on these envelope fields:

- `audit_id`
- `payment`
- `decision`
- `guardrails`
- `logs`

For finalized requests, `plan` is also present and should be shown in agent and
frontend surfaces as the typed ActionPlan that NeuroChain evaluated.

Important behavior:

- invalid payment proofs return `402 invalid_payment` without running the
  intent planner
- replayed payment signatures return `409 payment_replay_blocked`
- expired challenges return `402 payment_expired`
- paid requests still run the same guardrails:
  - `3` allowlist
  - `4` contract policy
  - `5` intent safety / typed slot error / low confidence
- `requires_approval` means payment finalized and guardrails passed, but the
  request stops before any submit/signing boundary until an owner or human
  approval step exists

Soroban v2 template example through the x402 gateway:

```powershell
$env:NC_CONTRACT_POLICY="examples/soroban_claim_rewards_template_policy.json"

$body = @{
  model     = "intent_stellar"
  prompt    = "Invoke contract rewards function claim_rewards for wallet GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX"
  threshold = 0.0
} | ConvertTo-Json

$challenge = Invoke-RestMethod `
  -Method Post `
  -Uri "http://127.0.0.1:8081/api/x402/stellar/intent-plan" `
  -ContentType "application/json" `
  -Body $body

Invoke-RestMethod `
  -Method Post `
  -Uri "http://127.0.0.1:8081/api/x402/stellar/intent-plan" `
  -ContentType "application/json" `
  -Headers @{ "PAYMENT-SIGNATURE" = "paid:$($challenge.challenge_id)" } `
  -Body $body
```

Expected finalized Soroban v2 behavior:

- `payment.state = "finalized"`
- `decision.status = "approved"`
- `guardrails.state = "passed"`
- `plan.actions[0].kind = "soroban_contract_invoke"`
- `plan.actions[0].function = "claim_rewards"`
- `logs` include `soroban_deep_template: expanded=true template=claim_rewards`

If the prompt matches the `claim_rewards` template but misses the wallet/account
slot, the paid request is still blocked by intent safety:

- `payment.state = "finalized"`
- `decision.status = "blocked"`
- `guardrails.exit_code = 5`
- `guardrails.reason = "intent_safety"`

The same x402-paid Soroban v2 path is also covered for higher-signal DeFi-like
templates:

- `deposit` approved:
  - prompt shape: `Invoke contract deposit function deposit 100 for wallet G...`
  - action: `soroban_contract_invoke`
  - function: `deposit`
  - args: `account`, `amount`, `asset`
- `deposit` missing amount:
  - finalized payment
  - blocked by intent safety
  - `guardrails.exit_code = 5`
- `swap` approved:
  - prompt shape: `Invoke contract swap function swap amount 100 from USDC to XLM min_out 95 for wallet G...`
  - action: `soroban_contract_invoke`
  - function: `swap`
  - args: `account`, `amount`, `from_asset`, `to_asset`, `min_out`
- `swap` missing `min_out`:
  - finalized payment
  - blocked by intent safety
  - `guardrails.exit_code = 5`

The x402 gateway also preserves contract-policy enforcement. For example, if a
paid request invokes a contract function that is not present in
`allowed_functions` while `contract_policy_enforce` is enabled, the payment is
finalized but the action is blocked:

```powershell
$env:NC_CONTRACT_POLICY="contracts/rewards/policy.json"

$body = @{
  model                   = "intent_stellar"
  prompt                  = "Invoke contract CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function emergency_withdraw args={""account"":""GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX""}"
  threshold               = 0.0
  contract_policy_enforce = $true
} | ConvertTo-Json
```

Expected finalized contract-policy block:

- `payment.state = "finalized"`
- `decision.status = "blocked"`
- `decision.reason = "contract_policy"`
- `guardrails.exit_code = 4`
- `guardrails.reason = "contract_policy"`

Static response fixtures for frontend and agent integrations live under:

```text
examples/x402_response_contract/
```

That directory now includes:

- `README.md` -> human-readable scenario matrix and field semantics
- `schema.json` -> machine-readable JSON Schema for the response envelope
- `types.ts` -> frontend-friendly TypeScript contract for agent/UI clients
- `client_adapter.ts` -> example mapper from backend response to UI/agent state
- `viewer.html` / `viewer.js` -> fixture and local live viewer for the agent-facing
  execution flow
- `*.json` -> concrete examples for `payment_required`, `approved`,
  `requires_approval`, `blocked_exit_3_allowlist`,
  `blocked_exit_4_contract_policy`, `blocked_exit_5_intent_safety`,
  `replay_blocked`, `expired`, and `invalid_payment`

The fixture test parses `schema.json` and validates every example against the
same required fields, types, enums, and x402 audit-id prefix used by the
frontend/agent contract.

The same test also checks that `types.ts` and the README client flow stay next
to the fixtures. Client integrations should follow the simple loop:
`payment_required -> retry with PAYMENT-SIGNATURE -> finalized decision`, then
render `decision`, `guardrails`, `logs`, and finalized `plan`.

`client_adapter.ts` shows the intended UI state mapping:

- `payment_required` -> show x402 challenge and retry affordance
- `approved` -> render the finalized ActionPlan
- `requires_approval` -> render the plan and guardrails, but do not submit,
  sign, or broadcast
- `blocked_allowlist` -> explain exit `3`
- `blocked_contract_policy` -> explain exit `4`
- `blocked_intent_safety` -> explain exit `5`
- `replay_blocked` / `expired` / `invalid_payment` -> ask for a fresh challenge

To inspect the static response viewer locally:

```powershell
python -m http.server 8787 -d examples/x402_response_contract
```

Then open:

```text
http://127.0.0.1:8787/viewer.html
```

The viewer also has a local live mode. Start the server separately:

```powershell
$env:NC_MODELS_DIR="models"
$env:NC_CONTRACT_POLICY="examples/soroban_claim_rewards_template_policy.json"
cargo run --bin neurochain-server
```

Then use the **Live x402 API** panel with the default base URL:

```text
http://127.0.0.1:8081
```

The live panel calls `/api/x402/stellar/intent-plan`, receives
`payment_required`, retries with the current mock
`PAYMENT-SIGNATURE: paid:<challenge_id>` proof, and renders the resulting
approved, requires-approval, or blocked envelope in the same UI. It remains
mock-only: no wallet signing, no submit/broadcast, and no real facilitator
settlement.

The live panel also has one-click presets for the current backend matrix:

- approved `claim_rewards`
- requires approval `claim_rewards`
- blocked exit `3` allowlist
- blocked exit `4` contract policy
- blocked exit `5` intent safety / missing slot
- replay blocked

Use these presets to verify that frontend state mapping still matches the
server response contract after x402 or guardrail changes.

The same matrix is covered by an automated API smoke test:

```powershell
cargo test --test server_analyze api_x402_stellar_live_preset_matrix_smoke -- --nocapture
```

Optional x402 server environment variables:

| Env var | Meaning | Default |
| --- | --- | --- |
| `NC_X402_STELLAR_AMOUNT` | Mock price amount | `0.01` |
| `NC_X402_STELLAR_ASSET` | Mock payment asset | `USDC` |
| `NC_X402_STELLAR_NETWORK` | Payment network label | `stellar:testnet` |
| `NC_X402_STELLAR_RECEIVER` | Receiver label/account placeholder | `mock-receiver` |
| `NC_X402_STELLAR_TTL_SECS` | Challenge lifetime | `300` |
| `NC_X402_STELLAR_VERIFIER` | Verifier boundary mode: `mock` for local development or `facilitator` for the explicit fail-closed verify/settle stub | `mock` |
| `NC_X402_STELLAR_AUDIT_PATH` | Optional safe JSONL audit output path | unset |
| `NC_X402_STELLAR_STORE_PATH` | Optional file-backed challenge/replay store path | unset |
| `NC_ENV` / `APP_ENV` / `RUST_ENV` | When set to `production`, disables the mock x402 verifier and fails closed until a real facilitator verifier is configured | unset |

When `NC_X402_STELLAR_AUDIT_PATH` is set, the server appends safe JSONL audit
rows for payment-required, finalized, blocked, replay, expired, and invalid
payment states. Audit rows intentionally do not store the raw
`PAYMENT-SIGNATURE` header or the mock `paid:<challenge_id>` signature.

By default, x402 challenge/replay state is in-memory and is reset when the
server restarts. When `NC_X402_STELLAR_STORE_PATH` is set, the server persists
the challenge state to a local JSON file so a challenge created before a restart
can still be finalized after the restart, and finalized challenges continue to
block replay after later restarts. The store persists challenge ids and payment
state, not the raw `PAYMENT-SIGNATURE` header or the mock `paid:<challenge_id>`
signature. If a configured file store cannot be read or parsed, x402 requests
fail closed with `state_unavailable` instead of silently falling back to
in-memory state. This is a dev/production-shape bridge, not a real facilitator
integration.

Implementation note:

- `src/x402_facilitator.rs` owns the payment verifier boundary
- current verifier kind: `mock` in development
- current boundary kind: `mock_header_store`
- `NC_X402_STELLAR_VERIFIER=facilitator` selects the explicit
  `facilitator_verify_settle` boundary, but it currently returns
  `state_unavailable` because real verify/settle transport is not implemented
- production envs disable the mock verifier; production payment requests stay
  fail closed until the facilitator transport is implemented and configured
- future real facilitator support should be added behind that verifier boundary
  while keeping `payment`, `decision`, `guardrails`, `logs`, `audit_id`, and
  finalized `plan` stable for clients

## 13) ZK Guardrail: Local Binding And Soroban Verification

The REPL exposes three deliberately separate boundaries. A valid result at one
boundary never silently advances to the next one.

### 13.1) Inspect The Public Binding Locally

```text
zk.demo approved
zk.demo requires_approval
zk.demo blocked
zk status
```

The bundled scenarios are available in both the local CLI REPL and the public
WebSocket demo. They decode the public journal, recompute the typed ActionPlan
hash, validate the journal digest and image binding, and display the decision.
They do not perform the Groth16 pairing check, so the output says
`cryptographic_verification: required_on_stellar`.

The local CLI REPL can also inspect caller-selected JSON artifacts:

```text
zk.verify action_plan="hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json" proof="hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"
```

`zk.verify` limits each input to a regular UTF-8 JSON file of at most 2 MiB.
The public WebSocket REPL sets `NC_STELLAR_REMOTE_REPL=1`, which disables this
file-reading form before either path is opened. Public users can run only the
bundled scenarios.

For a caller-selected ActionPlan and private policy, validate and prove an
untracked private input with the RISC Zero runner:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_risc0_e2e.ps1 `
  -InputPath C:\private\neurochain-zk-input.json `
  -CheckInput

powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/run_risc0_e2e.ps1 `
  -InputPath C:\private\neurochain-zk-input.json `
  -OutputPath C:\private\neurochain-zk-public-proof.json
```

The strict schema is documented by the public synthetic
`risc0/private_input.example.json`. Real policy rules, salts and audit nonces
must remain outside the repository. The generated proof artifact is public and
can be loaded with `zk.verify`, followed by `zk.stellar.verify last`.

### 13.2) Verify The Proof On Soroban Without Changing State

Configure the deployed application contract and a Stellar CLI source alias:

```text
network: testnet
wallet: nc-zk-demo
zk_contract: C...
zk.stellar.verify approved
zk.stellar.verify requires_approval
zk.stellar.verify blocked
```

The contract ID can instead come from `NC_ZK_GUARDRAIL_CONTRACT`. The REPL
invokes the contract's `verify` method with `--send no` and the configured
instruction leeway. Soroban then:

1. routes the genuine RISC Zero seal to the pinned Groth16 verifier
2. checks the evaluator image ID
3. checks that the owner authorized the journal's policy commitment/version
4. returns the typed decision without consuming the audit nullifier.

The REPL compares every returned binding and decision field with its local
ActionPlan/journal view before displaying `verified_on_stellar`. A mismatch
fails closed as exit `4`. Successful output always includes:

```text
mode: read_only_verification
cryptographic_verification: verified_on_stellar
authorized_private_policy: verified_on_stellar
nullifier_consumed: false
verification_transaction_submitted: false
underlying_action_submit_allowed: false
```

This command is repeatable because it does not write contract state. It is the
safe inspection step before an optional testnet transaction.

### 13.3) Submit A Real Testnet Proof-Verification Transaction

To produce public ledger evidence for a demo, use the explicit attest command
after configuring a funded testnet source:

```text
setup testnet
wallet_bootstrap: zk-video
zk.stellar.attest approved
```

`zk.stellar.attest` is hard-limited to the `testnet` network and requires flow
mode. It invokes the same permissionless `verify` contract method with
`--send yes`, waits for a new source-account transaction in Horizon, and prints
the transaction hash and a StellarExpert testnet link. The call does not use
`verify_and_consume`, does not consume the audit nullifier, and never submits
the ActionPlan represented by the proof.

Successful output includes:

```text
mode: submitted_testnet_attestation
cryptographic_verification: verified_on_stellar
nullifier_consumed: false
verification_transaction_submitted: true
transaction_hash: <64-character transaction hash>
stellar_expert_url: https://stellar.expert/explorer/testnet/tx/<hash>
underlying_action_submit_allowed: false
```

The command name is the explicit transaction action. `--no-flow` blocks it,
and selecting `public` or `mainnet` fails before Stellar CLI is invoked.

`zk status` preserves the latest successful Stellar result for the current
REPL session. After attestation it reports the validated local binding,
`stellar_verification: verified_on_stellar`, the verification mode, whether an
attestation transaction was submitted, its transaction hash, nullifier state,
and `underlying_action_submit_allowed: false`. Running a new local `zk.demo` or
`zk.verify` inspection clears the older Stellar result instead of presenting it
for a different proof.

### 13.4) Consume The Nullifier In A Separate Owner Transaction

The local operator can demonstrate persistent replay protection separately:

```text
zk.stellar.consume approved
```

This command is disabled in remote REPL mode. Locally it requires `--flow`, an
explicit confirmation unless `--yes` was intentionally supplied, and the
contract owner's source alias. It calls `verify_and_consume`, then reads
`is_consumed` back from Soroban. It stores only the audit nullifier; it does not
sign, submit, or broadcast the ActionPlan represented by the proof. A repeated
consume is contract error `3`, mapped to the existing exit `4` boundary.

### 13.5) Server Inspection Endpoint

The server exposes a separate inspection endpoint for the hackathon public
proof artifact:

```text
POST /api/stellar/zk-attestation/view
```

```powershell
$actionPlan = Get-Content -Raw hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json | ConvertFrom-Json
$proof = Get-Content -Raw hackathons/stellar-real-world-zk/fixtures/groth16_approved.json | ConvertFrom-Json
# Use groth16_requires_approval.json to inspect the proven approval boundary.
# Use groth16_blocked_exit_3.json to inspect the proven allowlist block.
$body = @{ action_plan = $actionPlan; proof = $proof } | ConvertTo-Json -Depth 10

Invoke-RestMethod `
  -Method Post `
  -Uri "http://127.0.0.1:8081/api/stellar/zk-attestation/view" `
  -ContentType "application/json" `
  -Body $body
```

This endpoint recomputes the canonical typed ActionPlan hash and validates the
public journal digest, image ID and decision semantics. It does not perform the
Groth16 cryptographic verification itself. A successful response therefore
still has:

- `zk_attestation.verification_state = "binding_validated"`
- `zk_attestation.cryptographically_verified = false`
- `zk_attestation.stellar_verification_required = true`
- `execution.state = "blocked"`
- `execution.submit_allowed = false`
- `execution.next_step = "verify_on_stellar_then_separate_approval"`

The genuine cryptographic proof path is also exercised by
`run_soroban_localnet_e2e.ps1`. It deploys the verifier, router and application,
first proves that read-only `verify` leaves the nullifier unused, then performs
the owner-authenticated consume and tests replay plus invalid-proof rejection.
Pass `-Scenario requires_approval` to verify the approval-threshold scenario;
it returns exit `0` with `NextStep::RequiresApproval`, not execution permission.
Pass `-Scenario blocked_allowlist` for decision `blocked`, exit `3` and
`NextStep::Blocked`.

An optional testnet deployment script is available at
`hackathons/stellar-real-world-zk/scripts/deploy_testnet.ps1`. It is restricted
to testnet and refuses to run without `-Execute`. A successful authorized run
writes the secret-free `deployments/testnet.json` manifest. The repository does
not claim a testnet deployment until that manifest exists.

Changing the ActionPlan, journal digest or journal bytes makes the API view fail
closed. `NC_API_KEY` protects this route when configured. The route never signs,
submits or broadcasts.
