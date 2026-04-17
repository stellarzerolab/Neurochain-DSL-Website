# Golden path (blocked variant): model-agnostic decision gate -> no payment (single .nc run)
#
# Run:
#   cargo run --release --bin neurochain-stellar -- examples/golden_path_model_agnostic_blocked.nc --flow
#
# This demonstrates the same structure as the golden path, but the gate intentionally
# evaluates to a non-matching label so the payment step is skipped.

network: testnet
wallet: nc-testnet

# Default gate profile (SST2 sentiment) - negative prompt, Positive gate target
AI: "models/distilbert-sst2/model.onnx"
set gate from AI: "This is terrible!"
set allow_label = "Positive"

if gate == allow_label:
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
else:
    neuro "Gate did not pass; payment skipped."
