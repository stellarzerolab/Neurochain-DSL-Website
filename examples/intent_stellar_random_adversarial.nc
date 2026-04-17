# IntentStellar adversarial random regression (seeded-style).
# Plan-only by default (no submit):
#   cargo run --release --bin neurochain-stellar -- examples/intent_stellar_random_adversarial.nc
#
# Optional flow:
#   cargo run --release --bin neurochain-stellar -- examples/intent_stellar_random_adversarial.nc --flow

network: testnet
wallet: nc-testnet
intent_threshold: 0.00
AI: "models/intent_stellar/model.onnx"

neuro "--- adversarial round2 start ---"

# TransferXLM (typos/noise)
set stellar intent from AI: "sendd 5 xIm to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ !!!"
set stellar intent from AI: "run payment 3.33 XLM to GC6FLLQELHZ3GXDXFIIMJ477E2FVEI2PFSM4AD4IXELERA4ZUUPVLBLQ and then check balance GC6FLLQELHZ3GXDXFIIMJ477E2FVEI2PFSM4AD4IXELERA4ZUUPVLBLQ"

# TransferAsset ambiguity pair
set stellar intent from AI: "trnsfer 4 USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5 to GC5IPWDT6CXXHPQVOETX2AHKABSSSYOSZFWGOLCRJM56AFMDSTMLV3KL"
set stellar intent from AI: "run send 7 TESTUSD:GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P and then check tx status aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

# Unknown with run/call/execute/trigger
set stellar intent from AI: "run weather forecast for helsinki"
set stellar intent from AI: "call support and open ticket"
set stellar intent from AI: "execute local backup process"
set stellar intent from AI: "trigger random quote generator"

# ContractInvoke (valid + invalid typed args)
set stellar intent from AI: "invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function transfer args={\"to\":\"GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ\",\"amount\":100} arg_types={\"to\":\"address\",\"amount\":\"u64\"}"
set stellar intent from AI: "execute transfer on CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ args={\"to\":\"World\",\"amount\":10} arg_types={\"to\":\"address\",\"amount\":\"u64\"}"
set stellar intent from AI: "call set_blob on CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ args={\"blob\":\"0xZZ11\"} arg_types={\"blob\":\"bytes\"}"

# Multi-intent lines
set stellar intent from AI: "check balance for GC5IPWDT6CXXHPQVOETX2AHKABSSSYOSZFWGOLCRJM56AFMDSTMLV3KL and then send 1 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
set stellar intent from AI: "run tx lookup deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef and then friendbot GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ"
set stellar intent from AI: "run hello on CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ and then send 1 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"

# Coverage for remaining labels
set stellar intent from AI: "create acccount GC6FLLQELHZ3GXDXFIIMJ477E2FVEI2PFSM4AD4IXELERA4ZUUPVLBLQ with startng balance 2 XLM"
set stellar intent from AI: "add trstline USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5 limt 700"
set stellar intent from AI: "trigger friendbot refill to GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ"
set stellar intent from AI: "check tx sttus deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"

neuro "--- adversarial round2 end ---"
