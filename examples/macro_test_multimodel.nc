AI: "models/intent_macro/model.onnx"

neuro "=== MACRO TEST MULTIMODEL START ==="

# This test validates the end-to-end multi-model integration:
# - The macro-intent model stays usable for `macro from AI:`
# - `AI:` switches the active classification model (toxicity/sentiment/factcheck/intent)
# - `set X from AI:` fills variables, and macros build deterministic DSL based on them

# (1) Toxicity ----------------------------------------------------------
neuro "--- TOXICITY MODEL ---"
AI: "models/toxic_quantized/model.onnx"

set toxicity from AI: "You are stupid"
# EXPECT: Warning
macro from AI: "If toxicity is Toxic output Warning else output OK"

set toxicity2 from AI: "Nice teamwork!"
# EXPECT: OK
macro from AI: "If toxicity2 is Toxic output Warning else output OK"

# (2) Sentiment ---------------------------------------------------------
neuro "--- SENTIMENT MODEL ---"
AI: "models/distilbert-sst2/model.onnx"

set mood from AI: "I love this movie."
# EXPECT: Great
macro from AI: "If mood is Positive say Great else say Bad"

set mood2 from AI: "This is terrible."
# EXPECT: Bad
macro from AI: "If mood2 is Positive say Great else say Bad"

# (3) FactCheck ---------------------------------------------------------
neuro "--- FACTCHECK MODEL ---"
AI: "models/factcheck/model.onnx"

# Note: FactCheck labels can differ from “human intuition”, so we use examples
# that are known to behave consistently with the bundled model (contradiction / entailment).

set fact from AI: "Birds fly. | Penguins fly."
# EXPECT: Contradiction detected
macro from AI: "If fact is contradiction say Contradiction detected else say OK"

set fact2 from AI: "Dogs bark. | Dogs can make noise."
# EXPECT: OK
macro from AI: "If fact2 is entailment say OK else say FAIL"

# (4) Intent (commands) -------------------------------------------------
neuro "--- INTENT MODEL ---"
AI: "models/intent/model.onnx"

set cmd from AI: "Stop now"
# EXPECT: Stopping process
macro from AI: "If cmd equals StopCommand say Stopping process else say Continue"

set cmd2 from AI: "Go forward"
# EXPECT: Continue
macro from AI: "If cmd2 equals StopCommand say Stopping process else say Continue"

# (5) Macro-modelin persistenssi ---------------------------------------
neuro "--- MACRO MODEL STILL ACTIVE ---"
# EXPECT: Ping (2 lines)
macro from AI: Show Ping 2 times

neuro "=== MACRO TEST MULTIMODEL END ==="
