# NeuroChain SST‑2 sentiment model – test session
#
# Model: 2-class sentiment (Positive / Negative)
# This script also exercises basic DSL features (if/elif/else, math on strings, comparisons).

AI: "models/distilbert-sst2/model.onnx"
neuro "Starting sentiment analysis tests"

# Test 0: Comment lines are ignored
// This is a comment
# This is also a comment
neuro "Comment lines were ignored successfully"

# Test 1: Case-insensitive comparison
set expected = "Positive"
set prediction from AI: "I love this movie."
if prediction == expected:
    neuro "Test 1 PASS: prediction matched expected (case-insensitive)"
else:
    neuro "Test 1 FAIL: prediction did not match expected"

# Test 2: Negative comparison
if prediction != "Negative":
    neuro "Test 2 PASS: prediction != 'Negative'"
else:
    neuro "Test 2 FAIL: prediction == 'Negative' (unexpected)"

# Test 3: Variable-to-variable comparison
set expected2 = "Positive"
if prediction == expected2:
    neuro "Test 3 PASS: prediction matched expected2"
else:
    neuro "Test 3 FAIL: prediction did not match expected2"

# Test 4: Wrong expectation
set wrong_expected = "Negative"
if prediction == wrong_expected:
    neuro "Test 4 FAIL: unexpected match (should not happen)"
else:
    neuro "Test 4 PASS: correctly rejected wrong expectation"

# Test 5: Positive AI classification
set a from AI: "This is amazing!"
neuro a
if a == "Positive":
    neuro "Test 5 PASS: 'a' is Positive"

# Test 6: Negative AI classification
set b from AI: "This is terrible."
if b == "Negative":
    neuro "Test 6 PASS: 'b' is Negative"

# Test 7: OR condition with AI results
set x from AI: "Awful support, never again."
set y from AI: "Such a joyful moment."
if x == "Negative" or y == "Positive":
    neuro "Test 7 PASS: OR condition evaluated as true"

# Test 8: AND condition with AI results
set p from AI: "Brilliant work!"
set q from AI: "I'm extremely satisfied."
if p == "Positive" and q == "Positive":
    neuro "Test 8 PASS: AND condition evaluated as true"

# Test 9: if/elif/else branches with an AI result
set test from AI: "I hate this."
if test == "Positive":
    neuro "Test 9: Positive message"
elif test == "Negative":
    neuro "Test 9: Negative message"
else:
    neuro "Test 9: Unknown classification"

# Test 10: String arithmetic (feature demo)
set sum = "2" + "3"
neuro sum
set diff = "10" - "4"
neuro diff
set mul = "5" * "2"
neuro mul
set div = "8" / "2"
neuro div
set rem = "10" % "3"
neuro rem

# Test 11: Comparison expressions
set greater = "6" > "3"
neuro greater
set less = "2" < "5"
neuro less
set ge = "10" >= "10"
neuro ge
set le = "4" <= "7"
neuro le

# Test 12: Numbers without quotes
set number = 123
neuro number

# Test 13: Combine variables in an expression
set s1 = "5"
set s2 = "3"
set sum2 = s1 + s2
neuro sum2
set diff2 = s1 - s2
neuro diff2

# Test 14: Convert number -> string by prefixing an empty string
set as_string = "" + number
neuro as_string

# Test 15: Use AI labels in simple scoring
set pos from AI: "This is very good."
set neg from AI: "This is terrible and awful."
set score = "1"
if pos == "Positive":
    set score = score + "1"
if neg == "Negative":
    set score = score + "1"
neuro score

# Test 16: AI + logical combination
set ai1 from AI: "I feel fantastic today!"
set ai2 from AI: "Worst experience ever."
if ai1 == "Positive" and ai2 == "Negative":
    neuro "Test 16 PASS: both extremes detected"

# Test 17: AI label -> boolean variable
set review1 from AI: "Perfect!"
set review2 from AI: "Really disappointing"
set is_good = review1 == "Positive"
set is_bad = review2 == "Negative"
neuro is_good
neuro is_bad

# Test 18: AI vs variable comparison (boolean)
set expected = "Positive"
set got from AI: "Wonderful product"
set check = got == expected
neuro check

neuro "All SST-2 tests completed!"
