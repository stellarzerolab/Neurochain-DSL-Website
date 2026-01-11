# 🧩 NeuroChain – MacroIntent (End-to-End)

NeuroChain’s “intent macro” converts a clear natural-language request into a deterministic DSL template.

Pipeline:

`macro from AI: ...` → `MacroIntent` (labels: Loop/Branch/Arith/Concat/RoleFlag/AIBridge/DocPrint/SetVar/Unknown) → deterministic template → parser → interpreter

## 1) Run macro examples

```bash
cargo run --release --bin neurochain -- examples/macro_test.nc
```

Also included: a seeded “random” regression script that uses more realistic user phrasing:

```bash
cargo run --release --bin neurochain -- examples/macro_test_random.nc
```

## 2) Example: macro → DSL → run

Save this as `my_macro.nc`:

```nc
AI: "models/intent_macro/model.onnx"

neuro "=== MACRO TUTORIAL START ==="

set name = "Joe"
set city = "Helsinki"
set score = 10

# Loop (without quotes)
macro from AI: Show Ping 3 times

# Branch (use quotes when it contains if/elif/else/and/or)
macro from AI: "If score >= 10 say Congrats else say Nope"

# SetVar / Arith
macro from AI: "Set x to 5 and print it"
macro from AI: "Create variable total = 3 + 4 and print it"

# Concat
macro from AI: "Print 'Hello ' + name"
macro from AI: "Print 'City: ' + city"

# Comment / DocPrint
macro from AI: "Write a comment that says 'main starts here' using //"

neuro "=== MACRO TUTORIAL END ==="
```

Run it:

```bash
cargo run --release --bin neurochain -- my_macro.nc
```

Example output:

```text
neuro: === MACRO TUTORIAL START ===
neuro: Ping
neuro: Ping
neuro: Ping
neuro: Congrats
neuro: x=5
neuro: total=7
neuro: Hello Joe
neuro: City: Helsinki
neuro: // main starts here
neuro: === MACRO TUTORIAL END ===
```

## 3) Threshold and safety

- `NC_INTENT_THRESHOLD` (default `0.35`): if the classifier score is below threshold, NeuroChain uses deterministic heuristics for template selection.
- Loop macros clamp the repeat count to `1..=12` to prevent output flooding.

## 4) Good prompts (best practices)

Macros work best when the prompt is clear and “structural”:

- **Loop**: no quotes, clear “X times”
  - `macro from AI: Show Ping 3 times`
- **Branch**: use quotes if it contains `if/elif/else/and/or`
  - `macro from AI: "If score >= 10 say Congrats else say Nope"`
- **SetVar/Arith**: use `set/create/store` + optionally `and print it`
  - `macro from AI: "Set x to 5 and print it"`
  - `macro from AI: "Create variable total = 3 + 4 and print it"`
- **Concat**: use `'...' + var` (single quotes in the prompt are OK; the DSL uses `"`).
  - `macro from AI: "Print 'City: ' + city"`

Rule of thumb: keep prompts readable English (no slang), and avoid deep nested quoting.

## 5) (Optional) See intent + generated DSL

PowerShell:

```powershell
$env:NEUROCHAIN_RAW_LOG="1"
cargo run --release --bin neurochain -- my_macro.nc
```

Bash (WSL / Linux):

```bash
NEUROCHAIN_RAW_LOG=1 cargo run --release --bin neurochain -- my_macro.nc
```

Then open `logs/macro_raw_latest.log` to see for each macro:

- `INTENT`: label + score
- `DSL`: the generated DSL that was executed

## 6) Note about `AIBridge`

`AIBridge` is intended as an “AI → client/UI” bridge. Right now it behaves safely (it does not generate new DSL), so it typically **prints/echoes the request** and has no side effects.

## 7) See also

- `docs/language.md` – DSL syntax basics
- `docs/models.md` – AI models and `set x from AI:` usage
- `docs/troubleshooting.md` – common issues (WSL/target/env)
