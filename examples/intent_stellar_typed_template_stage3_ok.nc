# IntentStellar typed template v2 stage3 demo (template-side `arg_types=` pass)
#
# Goal:
# - Show practical normalization in template path (without contract policy):
#   - symbol whitespace trim
#   - address lowercase/whitespace -> uppercase
#   - bytes separators + case -> normalized `0x...`
#   - u64 strings with `_` / `,` -> JSON number
#
# Run (plan-only):
#   cargo run --release --bin neurochain-stellar -- examples\\intent_stellar_typed_template_stage3_ok.nc

AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00

set stellar intent from AI: "Invoke contract CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":" World "} arg_types={"to":"symbol"}"
set stellar intent from AI: "Invoke contract CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={"to":" gcal4pifkwoifo6yt4t7tsses7sjcwv7hn7xautnffsgqk74rfusajbx ","blob":"0XDE AD_be-EF","ticker":" USDC ","amount":"1_000,000"} arg_types={"to":"address","blob":"bytes","ticker":"symbol","amount":"u64"}"
set stellar intent from AI: "Invoke contract CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={"to":"gcal4pifkwoifo6yt4t7tsses7sjcwv7hn7xautnffsgqk74rfusajbx","blob":"AA-BB","ticker":" XLM ","amount":"42"} arg_types={"to":"address","blob":"bytes","ticker":"symbol","amount":"u64"}"
