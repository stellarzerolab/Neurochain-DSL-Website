# Multi-model single-run pipeline:
# 1) classify sentiment with SST2
# 2) if positive -> switch to intent_stellar and build payment action
#
# Run:
#   cargo run --bin neurochain-stellar -- examples/multi_model_if_payment.nc --flow

AI: "models/distilbert-sst2/model.onnx"
network: testnet
wallet: nc-testnet
set mood from AI: "This is wonderful!"

if mood == "Positive":
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
else:
    neuro "Mood not positive, payment skipped"
