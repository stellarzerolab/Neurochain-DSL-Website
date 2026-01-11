# NeuroChain FactCheck model – test session
#
# Model: 3-class NLI (entailment / contradiction / neutral)

AI: "models/factcheck/model.onnx"
neuro "Starting FactCheck tests"

# Test 0: Comments are ignored
# This is a comment
// This is also a comment
neuro "Comment lines were ignored successfully"

# Test 1: Case-insensitive comparison against a variable
set expected = "Entailment"
set prediction from AI: "Books contain pages. | Books have pages."
if prediction == expected:
    neuro "Test 1 PASS: case-insensitive variable comparison OK"

# Test 2: entailment
set a from AI: "The Earth is a planet. | The Earth revolves around the Sun."
neuro a
if a == "entailment":
    neuro "Test 2 PASS: 'a' is entailment"

# Test 3: contradiction
set b from AI: "Paris is in France. | Paris is the capital of Germany."
if b == "contradiction":
    neuro "Test 3 PASS: 'b' is contradiction"

# Test 4: neutral
set c from AI: "Some people like pizza. | Pineapple on pizza is popular."
if c == "neutral":
    neuro "Test 4 PASS: 'c' is neutral"

# Test 5: OR condition
set x from AI: "The sky is blue. | The grass is blue."
set y from AI: "Birds fly. | Penguins fly."
if x == "contradiction" or y == "contradiction":
    neuro "Test 5 PASS: contradiction detected via OR"

# Test 6: AND condition
set p from AI: "Dogs bark. | Dogs can make noise."
set q from AI: "Water boils at 100C. | Water boils at 100 degrees Celsius."
if p == "entailment" and q == "entailment":
    neuro "Test 6 PASS: both are entailment"

# Test 7: Variable comparison (again)
set expected = "entailment"
set prediction from AI: "Books contain pages. | Books have pages."
if prediction == expected:
    neuro "Test 7 PASS: model responded as expected"

# Test 8: if/elif/else branches
set test from AI: "Paris is in France. | Paris is the capital of Germany."
if test == "entailment":
    neuro "Test 8: entailment"
elif test == "contradiction":
    neuro "Test 8: contradiction"
else:
    neuro "Test 8: neutral"

# Test 9: Arithmetic (feature demo)
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

# Test 10: String concatenation
set prefix = "Result: "
set value = "42"
set combined = prefix + value
neuro combined

# Test 11: Numbers without quotes
set number = 123
neuro number

# Test 12: Coerce number to string via empty prefix
set number_str = "" + number
neuro number_str

# Test 13: Comparison expressions
set greater = "6" > "3"
neuro greater
set less = "2" < "5"
neuro less
set ge = "10" >= "10"
neuro ge
set le = "4" <= "7"
neuro le

# Test 14: Score based on AI labels
set e1 from AI: "The moon is round. | The moon has a spherical shape."
set e2 from AI: "Cats bark. | Dogs bark."
set score = "0"
if e1 == "entailment":
    set score = score + "1"
if e2 == "contradiction":
    set score = score + "1"
neuro score

# Test 15: AI + logical combination
set ai1 from AI: "Plants use sunlight. | Plants photosynthesize."
set ai2 from AI: "Ice is hot. | Ice is very warm."
if ai1 == "entailment" and ai2 == "contradiction":
    neuro "Test 15 PASS: conditions evaluated correctly"

# Test 16: AI labels -> boolean variables
set check1 from AI: "The sun rises in the east. | The sun rises."
set check2 from AI: "Rain is dry. | Rain makes things wet."
set is_true = check1 == "entailment"
set is_false = check2 == "contradiction"
neuro is_true
neuro is_false

# Test 17: AI vs variable comparison (boolean)
set expected = "entailment"
set got from AI: "Computers can calculate. | Computers perform math operations."
set ok = got == expected
neuro ok

neuro "All FactCheck tests completed!"
