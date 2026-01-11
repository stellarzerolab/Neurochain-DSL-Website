AI: "models/distilbert-sst2/model.onnx"

# Values used for comparisons
set expected = "positive"        // lower-case on purpose
set sentiment from AI: "I love this movie."
neuro sentiment

# Test 1 – case-insensitive variable comparison
if sentiment == expected:
    neuro "Test 1 PASS: sentiment == expected (case-insensitive)"
else:
    neuro "Test 1 FAIL: sentiment != expected"

# Test 2 – negative comparison against a literal
if sentiment != "negative":
    neuro "Test 2 PASS: sentiment != 'negative'"
else:
    neuro "Test 2 FAIL: sentiment != 'negative'"

# Test 3 – compare two variables (different casing)
set expected2 = "Positive"
if sentiment == expected2:
    neuro "Test 3 PASS: sentiment == expected2"
else:
    neuro "Test 3 FAIL: sentiment != expected2"

# Test 4 – wrong expectation
set wrong_expected = "Negative"
if sentiment == wrong_expected:
    neuro "Test 4 FAIL: unexpected match (should not happen)"
else:
    neuro "Test 4 PASS: correctly rejected wrong expectation"

# Test 5 – numeric literals (decimals + unary minus)
set neg = -2
neuro neg
if neg < 0:
    neuro "Test 5 PASS: neg < 0"
else:
    neuro "Test 5 FAIL: neg not < 0"

set pi = 3.14
set total = pi + 0.86
neuro total
if total == 4:
    neuro "Test 6 PASS: decimals add"
else:
    neuro "Test 6 FAIL: decimals add"

set neg2 = -(2 + 3)
neuro neg2
if neg2 == -5:
    neuro "Test 7 PASS: unary minus parens"
else:
    neuro "Test 7 FAIL: unary minus parens"
