AI: "models/intent_macro/model.onnx"

neuro "=== MACRO TEST RANDOM START ==="

# Seeded + reproducible regression script (English-only prompts).
# Purpose: a broad but deterministic “random” set that you can diff across runs.
# Seed: 20260101
#
# Run (release):
#   cargo run --release --bin neurochain -- examples/macro_test_random.nc
#
# Debug tip:
#   $env:NEUROCHAIN_RAW_LOG="1"   (PowerShell)
#   $env:NEUROCHAIN_OUTPUT_LOG="1"
#

neuro "--- SEED: 20260101 ---"

# Base variables (for determinism)
set name = "Joe"
set city = "Helsinki"
set greeting = "Hi"
set target = "Team"
set title = "Status"
set body = "OK"
set status = "OK"
set product = "NeuroChain"

set x = 0
set y = 0
set a = 0
set b = 0
set score = 0
set attempts = 0
set temperature = 0
set battery = 0
set errors = 0
set warnings = 0
set points = 0
set n = 0
set result = 0

# --- 1) RANDOM LOOPS ---------------------------------------------------
neuro "--- RANDOM LOOPS ---"

macro from AI: Show Ping 2 times
macro from AI: Show car 4x
macro from AI: Show Done 13 times
macro from AI: "Say Hello twice"
macro from AI: "Repeat 'Alert' 3 times"
macro from AI: "Please echo the phrase 'OK' ten times"
macro from AI: "Please present 'NeuroChain ready' 6 times"
macro from AI: "Please output 'Hi' 7 times"
macro from AI: "Repeat 'test message' 9 times"
macro from AI: "Please announce 'I won' 1 time"
macro from AI: "Please display 'hello' five times"
macro from AI: "Run 8 times: reveal warning"
macro from AI: "Please repeat 'OK' three times"

# --- 2) RANDOM BRANCH --------------------------------------------------
neuro "--- RANDOM BRANCH ---"

set temperature = 10
macro from AI: "If temperature > 25, print Warm, else print Cold!!!"

set temperature = 30
macro from AI: "If temperature > 25, print Warm, else print Cold"

set score = 91
macro from AI: "If score >= 90 say Excellent, elif score >= 70 say Good, else say Needs work"

set score = 78
macro from AI: "If score >= 90 say Excellent, elif score >= 70 say Good, else say Needs work"

set score = 10
macro from AI: "If score equals 10, say Congrats, else say Nope"

set user = "guest"
macro from AI: "If user is not admin, print Access denied"

set user = "admin"
macro from AI: "If user is admin, print Access granted"

set battery = 15
macro from AI: "If battery < 20 print Low, elif battery < 50 print Medium, else print Full"

set battery = 35
macro from AI: "If battery < 20 print Low, elif battery < 50 print Medium, else print Full"

set battery = 90
macro from AI: "If battery < 20 print Low, elif battery < 50 print Medium, else print Full"

set attempts = 4
macro from AI: "If attempts > 3 print Locked else print Try again"

set attempts = 1
macro from AI: "If attempts > 3 print Locked else print Try again"

set score = 0
macro from AI: "If score <= 0 say Zero, otherwise say NonZero"

set score = 42
macro from AI: "If score <= 0 say Zero, otherwise say NonZero"

set x = 1
set y = 2
macro from AI: "If x equals 1 and y equals 2, say Match"

set a = 0
set b = 1
macro from AI: "If a is 0 or b is 1, print Skip"

set points = 7
macro from AI: "If points >= 10 say Gold, elif points >= 5 say Silver, else say Bronze"

set n = 3
macro from AI: "If n equals 1 say One, elif n equals 2 say Two, elif n equals 3 say Three, else say Other"
set flag = false
macro from AI: "If flag is true, say OK, else say ERROR"
set score = 11
macro from AI: "If score is greater than 10, say High, else say Low"

# --- 3) RANDOM SETVAR / ARITH / CONCAT --------------------------------
neuro "--- RANDOM SETVAR / ARITH / CONCAT ---"

macro from AI: "Set x to 5 and print it"
macro from AI: "Create variable total = 3 + 4 and print it"
macro from AI: "Set user_id = 42 and print it"

set a = 1
set b = 2
macro from AI: "Calculate (a + b) * 2 and store in r"
macro from AI: "Print the value of r"

set x = 10
set y = 2
macro from AI: "Subtract y from x, divide by 4, store in q"
macro from AI: "Print the value of q"

macro from AI: "Set remainder = 17 % 5 and print remainder"

macro from AI: "Print 'Hello ' + name"
macro from AI: "Print 'City: ' + city"
macro from AI: "Print greeting + ' ' + target"
macro from AI: "Join title + ': ' + body"

macro from AI: "Print 'Status: ' + status"
macro from AI: "Print 'Product: ' + product"

macro from AI: "Combine 'Data' and 'Ops' into label and print label"

# --- 4) RANDOM ROLEFLAG / AIBRIDGE -----------------------------------
neuro "--- RANDOM ROLEFLAG / AIBRIDGE ---"

macro from AI: "Set role = admin and print it"
macro from AI: "Set role = guest and print it"
macro from AI: "Forward model output to client"
macro from AI: "Bridge assistant output to UI"

# --- 5) RANDOM DOCPRINT / COMMENT / UNKNOWN ---------------------------
neuro "--- RANDOM DOCPRINT / COMMENT / UNKNOWN ---"

macro from AI: Say Hello world
macro from AI: Output Access denied
macro from AI: "Say the number 42"
macro from AI: "Format Hello and World with a comma."
macro from AI: "Write a comment that says 'main starts here' using //"
macro from AI: "Add comment # init block and print Starting"
macro from AI: "Insert comment // data loading then set load_done = true and print load_done"
macro from AI: "Add comment '# cleanup' and print Done"
macro from AI: "Tell me a joke"
macro from AI: "How are you doing?"
macro from AI: "What is the weather today?"
macro from AI: "Let us chat about sports"

neuro "=== MACRO TEST RANDOM END ==="
