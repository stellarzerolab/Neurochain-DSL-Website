# IntentStellar typed template v2 stage3 demo (template-side `arg_types=` blocked)
#
# Goal:
# - Show per-arg typed errors in one prompt (address / bytes / symbol / u64)
# - Flow blocks safely before simulate/submit (`exit 5`)
#
# Run (blocked flow demo):
#   cargo run --release --bin neurochain-stellar -- examples\\intent_stellar_typed_template_stage3_error.nc --flow --yes

AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00

set stellar intent from AI: "Invoke contract CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={"to":"World","blob":"0xABC","ticker":" BAD VALUE ","amount":"18446744073709551616"} arg_types={"to":"address","blob":"bytes","ticker":"symbol","amount":"u64"}"
