AI: "models/intent_macro/model.onnx"

neuro "=== MACRO TEST ROBUST START ==="

# Robustness / safety tests:
# - lots of punctuation and parentheses
# - unicode and “nasty” forms inside quotes
# - a longer prompt (should not break the DSL)
#
# Note: most punctuation is kept inside quotes so the lexer does not reject the line.

neuro "--- PUNCTUATION / QUOTES ---"

# EXPECT: Access denied
macro from AI: "Output 'Access denied'!!!"

# EXPECT: Hello, World
macro from AI: "Format Hello and World with a comma..."

# EXPECT: // main starts here
macro from AI: "Write a comment that says 'main starts here' using // (pls)"

neuro "--- PARENS / OPERATORS ---"

set a = 1
set b = 2
# EXPECT: 6
macro from AI: "Calculate (((a + b))) * 2 and store in r"
macro from AI: "Print the value of r"

set x = 10
set y = 2
# EXPECT: 2
macro from AI: "Subtract y from x, divide by 4, store in q (fast)"
macro from AI: "Print the value of q"

neuro "--- LONG PROMPT (SHOULD STILL WORK) ---"

set name = "Joe"
# EXPECT: Hello Joe
macro from AI: "Please, kindly, if possible: print 'Hello ' + name (no extra text) -- thanks!!!"

neuro "--- UNICODE (INSIDE QUOTES) ---"

# EXPECT: 😀 (2 lines)
macro from AI: "Say 😀 twice"

# EXPECT: ✅ OK (3 lines)
macro from AI: "Repeat '✅ OK' 3 times"

neuro "=== MACRO TEST ROBUST END ==="
