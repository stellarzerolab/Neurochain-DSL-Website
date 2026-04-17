# IntentStellar policy-backed typed v2 stage2 normalization demo (pass, multi-case)
#
# Goal:
# - Show several "user small mistakes" that stage2 normalization fixes:
#   - symbol whitespace trim
#   - address lowercase/whitespace -> uppercase
#   - bytes "0X..." / bare hex -> normalized "0x..."
#   - u64 strings ("00100", " 42 ") -> numbers
# - Result: actions remain `soroban_contract_invoke` (no `slot_type_error`)
#
# Run (plan-only, default file mode without --flow):
#   cargo run --release --bin neurochain-stellar -- examples\\intent_stellar_policy_typed_stage2_normalize.nc

AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
contract_policy: examples/intent_stellar_policy_typed_stage2_demo_policy.json

set stellar intent from AI: "Invoke contract CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":" World "}"
set stellar intent from AI: "Invoke contract CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={"to":"gcal4pifkwoifo6yt4t7tsses7sjcwv7hn7xautnffsgqk74rfusajbx","blob":"0X0A0B","ticker":" USDC ","amount":"00100"}"
set stellar intent from AI: "Invoke contract CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={"to":" gcal4pifkwoifo6yt4t7tsses7sjcwv7hn7xautnffsgqk74rfusajbx ","blob":"AABB","ticker":" XLM ","amount":" 42 "}"
