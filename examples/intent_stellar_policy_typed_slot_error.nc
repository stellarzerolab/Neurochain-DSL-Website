# IntentStellar policy-backed typed slot error demo (safe no-submit)
#
# Goal:
# - Contract policy requires `hello.to` to be a `symbol`
# - Prompt gives `args={"to":"Hello World"}` (invalid symbol due whitespace)
# - Result: slot_type_error -> Unknown -> flow blocked safely (exit 5)
#
# Run (PowerShell, release):
#   cargo run --release --bin neurochain-stellar -- examples\\intent_stellar_policy_typed_slot_error.nc --flow
#
# Expected:
# - ActionPlan contains `unknown`
# - warnings contain `slot_type_error`
# - flow prints "Intent safety guard blocked flow" (no preview/submit)

AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
contract_policy: contracts/CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ/policy.json

set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":"Hello World"}"
