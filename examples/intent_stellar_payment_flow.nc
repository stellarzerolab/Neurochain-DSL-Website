# IntentStellar payment example (script mode)
# Run:
#   cargo run --bin neurochain-stellar -- examples/intent_stellar_payment_flow.nc --flow

AI: "models/intent_stellar/model.onnx"
network: testnet
wallet: nc-testnet
txrep
set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
