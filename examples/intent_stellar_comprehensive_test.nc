# IntentStellar comprehensive test script (model-agnostic gate + full intent coverage)
#
# Safe default (plan-only, no submit):
#   cargo run --release --bin neurochain-stellar -- examples/intent_stellar_comprehensive_test.nc
#
# Full flow (simulate -> preview -> confirm -> submit):
#   cargo run --release --bin neurochain-stellar -- examples/intent_stellar_comprehensive_test.nc --flow
#
# Notes:
# - This file is designed to validate parser + model routing + guardrail behavior in one run.
# - Keep run_guardrail_negative = "off" for normal happy-path checks.
# - Turn run_guardrail_negative = "on" to verify safe no-submit/error paths.

network: testnet
wallet: nc-testnet
intent_threshold: 0.00
# debug
txrep

# Optional strict checks (off by default in this script):
# asset_allowlist: XLM,TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX
# soroban_allowlist: CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ
# allowlist_enforce

# -------------------------------------------------------------------
# 1) Model-agnostic gate (replace model/prompt/allow_label as needed)
# -------------------------------------------------------------------
AI: "models/distilbert-sst2/model.onnx"
set gate from AI: "This is wonderful!"
set allow_label = "Positive"

# Alternative gates:
# AI: "models/factcheck/model.onnx"
# set gate from AI: "Books contain pages. | Books have pages."
# set allow_label = "entailment"
#
# AI: "models/toxic_quantized/model.onnx"
# set gate from AI: "Great collaboration from everyone."
# set allow_label = "Not toxic"

if gate == allow_label:
    neuro "Gate passed -> running full IntentStellar matrix"
    AI: "models/intent_stellar/model.onnx"

    # ---------------------------------------------------------------
    # 2) Happy-path intent matrix (all main labels)
    # ---------------------------------------------------------------
    set stellar intent from AI: "Check balance for GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX asset XLM"
    set stellar intent from AI: "Create account GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ with starting balance 2"
    set stellar intent from AI: "Add trustline TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX limit 1000"
    set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
    set stellar intent from AI: "Send 12.5 TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ"
    set stellar intent from AI: "Fund testnet account GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX"
    set stellar intent from AI: "Check tx status f3eb378466903fc8eb132f67bc33519bb1233f5f78df4d9f0f6998a1445e5f15"
    set stellar intent from AI: 'Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":"World"}'

    # ---------------------------------------------------------------
    # 3) Manual action lines (parser parity check)
    # ---------------------------------------------------------------
    stellar.account.balance account="GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX" asset="XLM"
    stellar.change_trust asset_code="TESTUSD" asset_issuer="GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX" limit="1000"
    stellar.payment to="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" amount="1" asset_code="XLM"
    soroban.contract.invoke contract_id="CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ" function="hello" args={"to":"World"}

    # ---------------------------------------------------------------
    # 4) Optional negative guardrail checks
    # ---------------------------------------------------------------
    set run_guardrail_negative = "off"

    if run_guardrail_negative == "on":
        # Type validation failure -> slot_type_error -> Unknown -> safe no-submit in flow
        set stellar intent from AI: 'Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={"to":"World","amount":-1} arg_types={"to":"address","amount":"u64"}'
        # Low-confidence / non-financial prompt
        set stellar intent from AI: "Tell me a joke about stars"
else:
    neuro "Gate blocked -> intent matrix skipped"
