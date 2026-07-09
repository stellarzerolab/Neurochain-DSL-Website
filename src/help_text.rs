pub fn neurochain_language_help() -> &'static str {
    r#"
NeuroChain language — help

Basic syntax:
────────────────────────────────
AI: "path/to/model.onnx"        → Select an ONNX model
macro from AI: ...               → MacroIntent (intent → deterministic DSL template)
neuro "text"                     → Print a string
set x = "value"                  → Set a variable
set x from AI: "input"           → Run the active model into a variable
neuro x                          → Print a variable

Macros (intent → DSL):
────────────────────────────────
AI: "models/intent_macro/model.onnx"
macro from AI: Show Ping 3 times
macro from AI: "If score >= 10 say Congrats else say Nope"

Tip: if your prompt contains DSL keywords (`if/elif/else/and/or`), wrap it in quotes.
Loop macros clamp repeat counts to `1..=12` to prevent output flooding.

Control flow:
────────────────────────────────
if x == "value":
    neuro "..."                 → Runs when true

elif x != "value":
    neuro "..."                 → Additional condition

else:
    neuro "..."                 → Fallback branch

Logical operators:
────────────────────────────────
and, or                        → Example: if a == "X" and b != "Y":

Arithmetic:
────────────────────────────────
+  -  *  /  %                 → Example: set x = "4" + "2"
                               → To concat text + number: "" + number

Comparison operators:
────────────────────────────────
==  !=  <  >  <=  >=          → Example: if "3" > "1":
                               → Comparisons are case-insensitive

Variable expressions:
────────────────────────────────
set a = "5"
set b = "3"
set sum = a + b

Comments:
────────────────────────────────
# Comment                      → Ignored
// Comment                     → Also supported

Variables:
────────────────────────────────
If `neuro var` is not found in variables, the input is treated as a literal (fallback).

Supported AI models:
────────────────────────────────
SST2 (Sentiment): "Positive" / "Negative"
   set mood from AI: "This is amazing!"
   if mood == "Positive":
       neuro "Great"

Toxicity: "Toxic" / "Not toxic"
   set tox from AI: "You are bad."
   if tox == "Toxic":
       neuro "Warning"

FactCheck: "entailment" / "contradiction" / "neutral"
   set fact from AI: "Earth is flat. | Earth is round."
   if fact == "contradiction":
       neuro "Contradiction detected"

Intent: e.g. "GoCommand", "StopCommand", "LeftCommand"
   set cmd from AI: "Please stop."
   if cmd == "StopCommand":
       neuro "Stopping process"

IntentStellar: BalanceQuery/CreateAccount/ChangeTrust/TransferXLM/TransferAsset/FundTestnet/TxStatus/ContractInvoke/Unknown
   AI: "models/intent_stellar/model.onnx"
   set intent from AI: "Send 5 XLM to G..."
   if intent == "TransferXLM":
       neuro "Create payment action"

MacroIntent: Loop/Branch/Arith/Concat/RoleFlag/AIBridge/DocPrint/SetVar/Unknown
   AI: "models/intent_macro/model.onnx"
   macro from AI: Show Ping 3 times
   macro from AI: "If score >= 10 say Congrats else say Nope"

Run commands (CLI & server):
────────────────────────────────
# CLI (interpreter)
cargo run --bin neurochain
cargo run --release --bin neurochain -- examples/macro_test.nc

# REST API server
cargo run --bin neurochain-server
cargo run --release --bin neurochain-server

# Stellar demo API server
cargo run --bin neurochain-stellar-demo-server
cargo run --release --bin neurochain-stellar-demo-server

Optional logging:
────────────────────────────────
NEUROCHAIN_OUTPUT_LOG=1       → write `neuro:` output to a file (logs/run_latest.log)
NEUROCHAIN_RAW_LOG=1          → write intent/DSL debug to a file (logs/macro_raw_latest.log)

Docs & examples: https://github.com/stellarzerolab/Neurochain-DSL-Stellar
"#
}
