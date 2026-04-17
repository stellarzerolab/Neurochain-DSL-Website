# IntentStellar policy-backed typed slot OK demo (pass)
#
# Pair with:
# - examples/intent_stellar_policy_typed_slot_error.nc (fail -> slot_type_error / exit 5)
#
# Goal:
# - Contract policy requires `hello.to` to be a `symbol`
# - Prompt gives `args={"to":"World"}` (valid symbol)
# - Result: action remains `soroban_contract_invoke` (no `slot_type_error`)
#
# Run (plan-only, default file mode without --flow):
#   cargo run --release --bin neurochain-stellar -- examples\\intent_stellar_policy_typed_slot_ok.nc

AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
contract_policy: contracts/CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ/policy.json

set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":"World"}"
