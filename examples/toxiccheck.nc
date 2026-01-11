# NeuroChain toxicity model – test session
#
# Model: 2-class toxicity (Toxic / Not toxic)
# Supports both `#` and `//` comments.

AI: "models/toxic_quantized/model.onnx"
neuro "Starting toxicity model tests"

# Test 0: Comments are ignored
# This is a comment
// This is also a comment
neuro "Comment lines were ignored successfully"

# Test 1: Direct toxic message
set a from AI: "You suck"
neuro a
if a == "Toxic":
    neuro "Test 1 PASS: message 'a' is Toxic"
else:
    neuro "Test 1 FAIL: message 'a' was expected to be Toxic"

# Test 2: Non-toxic message
set b from AI: "Nice teamwork!"
if b != "Toxic":
    neuro "Test 2 PASS: message 'b' is not Toxic"

# Test 3: AND condition for two toxic messages
set x from AI: "You're garbage"
set y from AI: "You are a loser"
if x == "Toxic" and y == "Toxic":
    neuro "Test 3 PASS: both messages are Toxic"

# Test 4: OR condition for two non-toxic messages
set m from AI: "You're awesome"
set n from AI: "Great play!"
if m == "Toxic" or n == "Toxic":
    neuro "Test 4 FAIL: at least one message is Toxic"
else:
    neuro "Test 4 PASS: neither message is Toxic"

# Test 5: Case-insensitive + variable comparison
set expected = "not toxic"
set reply from AI: "I love your playstyle"
if reply == expected:
    neuro "Test 5 PASS: model reply matched expected (case-insensitive)"
if reply != "Toxic":
    neuro "Test 5 PASS: reply != Toxic"

# Test 6: if/elif/else
set feedback from AI: "Nice shot!"
if feedback == "Toxic":
    neuro "Test 6 FAIL: unexpected Toxic"
elif feedback == "Not toxic":
    neuro "Test 6 PASS: good feedback detected"
else:
    neuro "Test 6: unknown label"

# Test 7: Arithmetic (feature demo)
set s1 = "5"
set s2 = "2"
set sum = s1 + s2
neuro sum
set diff = s1 - s2
neuro diff
set mul = s1 * s2
neuro mul
set div = s1 / s2
neuro div
set rem = s1 % s2
neuro rem

# Test 8: String concatenation
set prefix = "Reply: "
set label = "Not toxic"
set combined = prefix + label
neuro combined

# Test 9: Comparison expressions
set greater = "5" > "2"
neuro greater
set less = "2" < "5"
neuro less
set gte = "5" >= "5"
neuro gte
set lte = "2" <= "5"
neuro lte

# Test 10: Numbers without quotes
set number = 123
neuro number

# Test 11: AI + simple scoring
set v1 from AI: "Amazing performance!"
set v2 from AI: "Idiot!"
set points = "0"
if v1 == "Not toxic":
    set points = points + "1"
if v2 == "Toxic":
    set points = points + "1"
neuro points

# Test 12: Boolean variables
set ok1 = v1 == "Not toxic"
set ok2 = v2 == "Toxic"
neuro ok1
neuro ok2

# Test 13: AI vs. variable comparison (boolean)
set expected = "Toxic"
set got from AI: "You are trash"
set check = got == expected
neuro check

neuro "All toxicity model tests completed!"
