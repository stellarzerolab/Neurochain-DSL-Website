# NeuroChain command intent model – test session
#
# Model: 7-class intent
# Labels: RightCommand, LeftCommand, UpCommand, DownCommand, GoCommand, StopCommand, OtherCommand

AI: "models/intent/model.onnx"
neuro "Starting intent model tests"

# Test 0: Comments are ignored
// This is a comment
# This is also a comment
neuro "Comment lines were ignored"

# Test 1: Right command
set a from AI: "Turn right"
neuro a
if a == "RightCommand":
    neuro "Test 1 PASS: command 'a' recognized as RightCommand"

# Test 2: Left command
set b from AI: "Go left"
if b == "LeftCommand":
    neuro "Test 2 PASS: command 'b' recognized as LeftCommand"

# Test 3: Up command
set c from AI: "Climb up"
if c == "UpCommand":
    neuro "Test 3 PASS: command 'c' recognized as UpCommand"

# Test 4: Down command
set d from AI: "Descend"
if d == "DownCommand":
    neuro "Test 4 PASS: command 'd' recognized as DownCommand"

# Test 5: Go command
set e from AI: "Go forward"
if e == "GoCommand":
    neuro "Test 5 PASS: command 'e' recognized as GoCommand"

# Test 6: Stop command
set f from AI: "Stop now"
if f == "StopCommand":
    neuro "Test 6 PASS: command 'f' recognized as StopCommand"

# Test 7: Irrelevant input
set g from AI: "How are you?"
if g == "OtherCommand":
    neuro "Test 7 PASS: command 'g' recognized as OtherCommand"

# Test 8: Case-insensitive variable comparison
set expected = "gocommand"
set prediction from AI: "Go forward"
if prediction == expected:
    neuro "Test 8 PASS: case-insensitive match OK"

# Test 9: Variable comparison (StopCommand)
set expected = "StopCommand"
set reply from AI: "Stop now"
if reply == expected:
    neuro "Test 9 PASS: variable comparison OK"

# Test 10: OR condition
set x from AI: "Fly up"
set y from AI: "Cease now"
if x == "UpCommand" or y == "StopCommand":
    neuro "Test 10 PASS: OR condition evaluated as true"

# Test 11: AND condition
set m from AI: "Shift right"
set n from AI: "Turn left"
if m == "RightCommand" and n == "LeftCommand":
    neuro "Test 11 PASS: both directions detected (Right & Left)"

# Test 12: if / elif / else
set test from AI: "Begin"
if test == "GoCommand":
    neuro "Test 12: Go"
elif test == "StopCommand":
    neuro "Test 12: Stop"
else:
    neuro "Test 12: Other command: " + test

# Test 13: Arithmetic (feature demo)
set sum = "2" + "2"
neuro sum
set diff = "5" - "1"
neuro diff
set mul = "4" * "2"
neuro mul
set div = "10" / "2"
neuro div
set rem = "9" % "4"
neuro rem

# Test 14: String concatenation
set prefix = "Command was: "
set cmd = "RightCommand"
set combined = prefix + cmd
neuro combined

# Test 15: Comparison expressions
set gt = "7" > "3"
neuro gt
set lt = "2" < "5"
neuro lt
set gte = "8" >= "8"
neuro gte
set lte = "6" <= "7"
neuro lte

# Test 16: Numbers without quotes
set number = 42
neuro number

# Test 17: AI label + boolean variables
set ai1 from AI: "Go now"
set ai2 from AI: "Stop there"
set score = "1"
if ai1 == "GoCommand":
    set score = score + "1"
if ai2 == "StopCommand":
    set score = score + "1"
neuro score

# Test 18: Boolean comparison variables
set one = ai1 == "GoCommand"
set two = ai2 == "StopCommand"
neuro one
neuro two

# Test 19: Variable comparison (another command)
set expected = "RightCommand"
set got from AI: "Turn right"
set ok = got == expected
neuro ok

# Test 20: Undefined variable handling (should not crash)
set y = "" + undefined_var
neuro y  # Expected: error printed about 'undefined_var' being undefined

neuro "All intent tests completed!"
