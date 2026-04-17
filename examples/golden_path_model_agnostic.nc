# Golden path: model-agnostic decision gate -> Stellar intent action (single .nc run)
#
# Run:
#   cargo run --release --bin neurochain-stellar -- examples/golden_path_model_agnostic.nc --flow
#
# The gate structure is model-agnostic:
#   set <var> from AI + if + set stellar intent from AI
# Only the model/prompt/allow_label pair changes.

network: testnet
wallet: nc-testnet

# Default gate profile (SST2 sentiment)
AI: "models/distilbert-sst2/model.onnx"
set gate from AI: "This is wonderful!"
set allow_label = "Positive"

# Alternative gate profile (FactCheck / NLI)
# AI: "models/factcheck/model.onnx"
# set gate from AI: "Books contain pages. | Books have pages."
# set allow_label = "entailment"

# Alternative gate profile (Toxic)
# AI: "models/toxic_quantized/model.onnx"
# set gate from AI: "Nice teamwork!"
# set allow_label = "Not toxic"

if gate == allow_label:
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"
else:
    neuro "Gate did not pass; payment skipped."
