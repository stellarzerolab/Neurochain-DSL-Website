# 📘 NeuroChain DSL – Language Guide

This document is the “teaching version” of NeuroChain’s DSL. The goal is that you can read and write `.nc` scripts without knowing the codebase.

If you just want end-to-end runnable scripts, start here:
- `docs/getting_started.md`
- `docs/macros.md`
- `docs/models.md`

## Quick cheat sheet

```nc
AI: "models/distilbert-sst2/model.onnx"     # select an ONNX model
set mood from AI: "I love this movie."     # run the active model into a variable
neuro mood                                 # print a variable

set x = 5                                  # numbers need no quotes
set name = "Joe"                           # strings use double quotes
set total = x + 2                          # arithmetic (+ - * / %)

if total >= 7:                             # if/elif/else use ':' and indentation
    neuro "OK"
else:
    neuro "NO"

AI: "models/intent_macro/model.onnx"

macro from AI: Show Ping 3 times           # MacroIntent (intent → deterministic DSL → run)
```

## 1) Basics

A NeuroChain script is a plain text file executed line-by-line: commands, variables, and control flow.

### Running

- Run a file (recommended for examples/regressions):
  - `cargo run --release --bin neurochain -- examples/macro_test.nc`
- Start the interactive CLI:
  - `cargo run --bin neurochain`

Note: NeuroChain also has a REST server. See `docs/getting_started.md` for how to run:
- `cargo run --bin neurochain-server`

Interactive commands:
`help`, `about`, `version`, `exit`.

### Comments

```nc
# This is a comment
// This is also a comment
```

Comments are ignored safely by the parser and can appear anywhere (including inside indented blocks).

### Printing: `neuro`

```nc
neuro "Hello!"
```

You can also print a variable:

```nc
set name = "Joe"
neuro name
```

Tip: if you want to print multiple words as a literal, you must use quotes.  
Without quotes (`neuro Hello world`) the parser treats it as separate tokens.

### Variables: `set`

```nc
set x = 5
set city = "Helsinki"
```

NeuroChain also supports simple expressions:

```nc
set a = 3
set b = 4
set total = a + b
neuro total
```

Supported operators: `+ - * / %`

- Numbers: calculated numerically (when both sides look like numbers).
- Strings: `+` concatenates strings (when at least one side is not a number).

Parentheses are supported in expressions:

```nc
set a = 3
set b = 4
set total = (a + b) * 2
neuro total
```

### Identifiers (variable names)

- Must start with a letter.
- Can contain letters, numbers, and `_`.
- Case-sensitive (`score` and `Score` are different).
- Avoid reserved keywords: `set`, `neuro`, `if`, `elif`, `else`, `and`, `or`, `AI`, `macro`, `from`.

### Values: strings, numbers, booleans, `None`

- Strings use **double quotes**: `"Hello"`, `"Helsinki"`.
- Numbers use no quotes: `42`, `-2`, `3.14`.
- Booleans: `true` / `false`
- Null-like value: `None`

```nc
set flag = true
set status = None
neuro flag
neuro status
```

### Undefined variables (robust behavior)

If you print a variable that does not exist, NeuroChain treats it as a literal string instead of crashing:

```nc
neuro undefined_variable
```

This is intentionally robust/safe for user scripts. (If you want stricter behavior later, that can be added as an opt-in mode.)

### Conditions: `if / elif / else`

```nc
set score = 10
if score >= 10:
    neuro "OK"
else:
    neuro "NO"
```

Supported comparisons: `== != < > <= >=`  
Supported boolean operators: `and`, `or`

```nc
set x = 1
set y = 2
if x == 1 and y == 2:
    neuro "Match"
```

### Comparison semantics (important)

- String comparisons are **case-insensitive** and **trim whitespace**.
  - `"OK" == "ok"` is true
  - `"Hello" == "Hello "` is true
- Numeric comparisons are numeric when both sides parse as numbers.

Variable-to-variable comparisons are supported:

```nc
set a = "OK"
set b = "ok"
if a == b:
    neuro "Same"
```

### Indentation (important)

`if/elif/else` blocks are Python-style: the line ends with `:` and the following lines are indented.

- Use **4 spaces** (no tabs).
- Keep all lines in a block at the same indentation level.
- When the block ends, indentation returns to the previous level.

```nc
if score >= 10:
    neuro "OK"
    neuro "Still OK"
else:
    neuro "NO"
```

### Strings vs numbers

- `+` becomes numeric addition if **both sides** look numeric.
  - `"4" + "2"` becomes `6` (because both parse as numbers)
- Otherwise `+` becomes concatenation.
  - `"City: " + city` becomes `"City: Helsinki"`

## 2) AI models: `AI:` and `set x from AI: ...`

You can use classification models directly in scripts:

```nc
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "I love this movie."
if mood == "Positive":
    neuro "Great"
else:
    neuro "Bad"
```

Tip: always use quotes for `set x from AI:` prompts, because they are usually multi-word.

You can switch models mid-script by setting `AI:` again. See per-model examples:
- `examples/distilbert-sst2check.nc`
- `examples/toxiccheck.nc`
- `examples/factcheck.nc`
- `examples/intentcheck.nc`

## 3) Macros: `macro from AI: ...` (intent → DSL)

Macros are a fast way to describe an action in natural language, which NeuroChain converts into a deterministic DSL template.

### Select the macro model

The MacroIntent model must be loaded (or it will be auto-loaded from the default path):

```nc
AI: "models/intent_macro/model.onnx"
```

Supported macro intents:

`Loop`, `Branch`, `Arith`, `Concat`, `RoleFlag`, `AIBridge`, `DocPrint`, `SetVar`, `Unknown`

### Loop macro

Best format for loops is without quotes:

```nc
macro from AI: Show Ping 3 times
```

Safety limit: the repeat count is clamped to `1..=12`.

### Branch macro

If your prompt contains control-flow keywords (`if/elif/else/and/or`), put it in quotes:

```nc
set score = 10
macro from AI: "If score >= 10 say Congrats else say Nope"
```

### SetVar / Arith / Concat (examples)

```nc
macro from AI: "Set x to 5 and print it"
macro from AI: "Create variable total = 3 + 4 and print it"
macro from AI: "Print 'Hello ' + name"
```

### DocPrint / comment

```nc
macro from AI: "Say the number 42"
macro from AI: "Format Hello and World with a comma"
macro from AI: "Write a comment that says 'main starts here' using //"
```

## 4) Debug & logs (optional)

You can log the macro “intent → DSL” path into files:

- `NEUROCHAIN_RAW_LOG=1` → `logs/macro_raw_latest.log` (intent + generated DSL)
- `NEUROCHAIN_OUTPUT_LOG=1` → `logs/run_latest.log` (all `neuro:` output)

Macro intent threshold:

- `NC_INTENT_THRESHOLD` (default `0.35`)

## 5) Common issues (quick fixes)

- **“Missing quote”**: strings in DSL must use `"..."` (not `'...'`).
- **Condition doesn’t work**: check the trailing `:` and indentation (4 spaces).
- **Macro prints the prompt**: classification may be `Unknown` or below threshold → use a clearer prompt or adjust `NC_INTENT_THRESHOLD`.
- **Unexpected numeric addition**: if both sides look numeric, `+` becomes math (`"4" + "2" → 6`).

## 6) See also

- `docs/getting_started.md` – end-to-end: Hello + variables + if
- `docs/macros.md` – end-to-end: macro → DSL → run
- `docs/models.md` – AI models and `set x from AI:` usage
- `docs/troubleshooting.md` – common issues (WSL/target/env)
