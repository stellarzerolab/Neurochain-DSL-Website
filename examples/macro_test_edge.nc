AI: "models/intent_macro/model.onnx"

neuro "=== MACRO TEST EDGE START ==="

# Edge/regression script.
# Goals:
# - Expand loop/branch/arith/concat/docprint variations
# - Ensure there are no parse/panic issues
# - Keep clear expectations in comments (not printed during execution)

# Base variables (for determinism)
set result = 0
set counter = 0
set value = 0
set x = 0
set y = 0
set a = 0
set b = 0
set score = 0
set attempts = 0
set level = 0
set points = 0
set warnings = 0
set errors = 0
set stage = "plan"
set status = "ok"
set role = "guest"
set temp = -2
set ready = false
set final_score = 123

# --- 1) Loop variants / unicode ---------------------------------------
neuro "--- LOOP VARIANTS ---"

# EXPECT: 😀 (2 lines)
macro from AI: "Say 😀 twice"

# EXPECT: 🚀 (3 lines)
macro from AI: "Say 🚀 3 times"

# EXPECT: Ping (4 lines)
macro from AI: "Output Ping 4x"

# EXPECT: hello (5 lines)
macro from AI: "Display hello 5 times"

# EXPECT: ⚠️ Alert (2 lines)
macro from AI: "Repeat '⚠️ Alert' 2 times"

# EXPECT: ✅ OK (10 lines)
macro from AI: "Kindly echo the phrase '✅ OK' ten times"

# EXPECT: NeuroChain ready (6 lines)
macro from AI: "Loop: Present 'NeuroChain ready' for 6 times"

# EXPECT: 👋 (7 lines)
macro from AI: "Please output 👋 7 times"

# EXPECT: warning (8 lines)
macro from AI: "Run 8 times: Reveal warning"

# EXPECT: test message (9 lines)
macro from AI: "Repeat 'test message' 9 times"

# EXPECT: I won (1 line)
macro from AI: "Please announce 'I won' 1 time"

# --- 2) Branch (numeric) / elif / else --------------------------------
neuro "--- BRANCH (NUMERIC) ---"

set temperature = 30
# EXPECT: Warm
macro from AI: "If temperature > 25, print Warm, else print Cold"

set score = 78
# EXPECT: Good
macro from AI: "If score >= 90 say Excellent, elif score >= 70 say Good, else say Needs work"

set battery = 15
# EXPECT: Low
macro from AI: "If battery < 20 print Low, elif battery < 50 print Medium, else print Full"

set warnings = 2
# EXPECT: Investigate warnings
macro from AI: "If warnings == 0 print All clear, else print Investigate warnings"

set errors = 0
# EXPECT: No errors
macro from AI: "If errors == 0 print No errors, else print Errors found"

set stage = "plan"
# EXPECT: Planning
macro from AI: "If stage == plan print Planning, elif stage == build print Building, else print Launching"

# “otherwise” alias
set score = 0
# EXPECT: Zero
macro from AI: "If score <= 0 say Zero, otherwise say NonZero"

# --- 3) Branch (boolean) / and-or -------------------------------------
neuro "--- BRANCH (BOOLEAN / AND-OR) ---"

set flag = false
# EXPECT: ERROR
macro from AI: "If flag is true, say OK, else say ERROR"

set flag = true
# EXPECT: OK
macro from AI: "If flag is true, say OK, else say ERROR"

set x = 1
set y = 2
# EXPECT: Match
macro from AI: "If x equals 1 and y equals 2, say Match"

set a = 0
set b = 1
# EXPECT: Skip
macro from AI: "If a is 0 or b is 1, print Skip"

set points = 7
# EXPECT: Silver
macro from AI: "If points >= 10 say Gold, elif points >= 5 say Silver, else say Bronze"

# Multi-elif (regressio)
set n = 3
# EXPECT: Three
macro from AI: "If n equals 1 say One, elif n equals 2 say Two, elif n equals 3 say Three, else say Other"

# --- 4) SetVar / Arith / Concat ---------------------------------------
neuro "--- SETVAR / ARITH / CONCAT ---"

macro from AI: "Set x to 5"
# EXPECT: 5
macro from AI: "Print the value of x"

set a = 1
set b = 2
# EXPECT: 6
macro from AI: "Calculate (a + b) * 2 and store in r"
macro from AI: "Print the value of r"

set x = 10
set y = 2
# EXPECT: 2
macro from AI: "Subtract y from x, divide by 4, store in q"
macro from AI: "Print the value of q"

set name = "Joe"
set score = 10
# EXPECT: Joe10
macro from AI: "Concatenate name and score with '+' and store in result"
macro from AI: "Print the value of result"

# EXPECT: Hello Joe
macro from AI: "Print 'Hello ' + name"

set greeting = "Hi"
set target = "Team"
# EXPECT: Hi Team
macro from AI: "Print greeting + ' ' + target"

# --- 5) DocPrint / Comment --------------------------------------------
neuro "--- DOCPRINT / COMMENT ---"

# EXPECT: Access denied
macro from AI: "Output 'Access denied'"

# EXPECT: 42
macro from AI: "Say the number 42"

# EXPECT: Hello, World
macro from AI: "Format Hello and World with a comma"

# EXPECT: // main starts here
macro from AI: "Write a comment that says 'main starts here' using //"

# --- 6) Negative / unsupported (should not crash) ---------------------
neuro "--- NEGATIVE / UNSUPPORTED (should not crash) ---"

set x = 3
set y = 3
# EXPECT: 0
macro from AI: "Set r to (x - y) ** 2"
macro from AI: "Print the value of r"

neuro "=== MACRO TEST EDGE END ==="
