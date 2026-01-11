AI: "models/intent_macro/model.onnx"

neuro "=== MACRO TEST START ==="

# --- 1) Loop macros ---------------------------------------------------
neuro "--- LOOP TESTS ---"

macro from AI: Show Ping 2 times
macro from AI: Show car 2 times
macro from AI: Say Hello 3 times

# More loops with different counts
macro from AI: Show Ping 1 time
macro from AI: Show Ping 5 times
macro from AI: Show Ping 7 times

# --- 2) Simple if macros ---------------------------------------------
neuro "--- IF TESTS ---"

set score = 10
macro from AI: "If score equals 10, say Congrats"

set user = "guest"
macro from AI: "If user is not admin, print Access denied"

set level = 5
macro from AI: "If level >= 5, output Proceed"

set flag = true
macro from AI: "If flag is true, print OK, else print ERROR"

# More if macros (coverage)
set score = 0
macro from AI: "If score equals 0, say Zero"

set score = 42
macro from AI: "If score is not 0, say NonZero"

set mode = "debug"
macro from AI: "If mode is debug, print Debug on, else print Debug off"

set user = "admin"
macro from AI: "If user is admin, print Access granted"

# --- 3) Variables + output + comment --------------------------------
neuro "--- VAR + COMMENT TESTS ---"

macro from AI: "Show how to set x to 5 and print it"
macro from AI: "Store 'hello' in greeting and echo it"
macro from AI: "Write a comment that says 'main starts here' using //"

# More var examples: arithmetic and concatenation
macro from AI: "Create variable total = 3 + 4 and print it"
macro from AI: "Set name to 'Joe' and print 'Hello ' + name"

# --- 4) Base macro (toxicity) ----------------------------------------
neuro "--- CORE MACRO TEST ---"

macro from AI: "Indicate the AI detected toxicity"

# --- 5) Elif / nested tests ------------------------------------------
neuro "--- ELIF / NESTED TESTS ---"

set temperature = 30
macro from AI: "If temperature > 25, print Warm, else print Cold"

set score = 78
macro from AI: "If score >= 90 say Excellent, elif score >= 70 say Good, else say Needs work"

set battery = 15
macro from AI: "If battery < 20 print Low, elif battery < 50 print Medium, else print Full"

set weather = "rainy"
macro from AI: "If weather equals rainy, print Take umbrella, else print Enjoy sun"

set priority = "high"
macro from AI: "If priority == high print Escalate, elif priority == medium print Monitor, else print Queue"

set attempts = 1
macro from AI: "If attempts > 3 print Locked, else print Try again"

# --- 6) Arithmetic + assignments -------------------------------------
neuro "--- MATH + ASSIGN TESTS ---"

macro from AI: "Create counter = 1 + 4 and print counter"
macro from AI: "Set balance = 100 - 35 and print balance"
macro from AI: "Set area = 6 * 7 and print area"
macro from AI: "Set quotient = 21 / 3 and print quotient"
macro from AI: "Set remainder = 17 % 5 and print remainder"
macro from AI: "Combine 'Data' and 'Ops' into label and print label"

# --- 7) String formatting --------------------------------------------
neuro "--- STRING FORMAT TESTS ---"

macro from AI: "Store 'Joe' in name and print Hello, name"
macro from AI: "Store 'Helsinki' in city and print 'City: ' + city"
macro from AI: "Set greeting = 'Hi' and target = 'Team', then print greeting + ' ' + target"

set status = "OK"
macro from AI: "Print 'Status: ' + status"

set product = "NeuroChain"
macro from AI: "Print 'Product: ' + product"

# --- 8) Comment macros ------------------------------------------------
neuro "--- COMMENT TESTS ---"

macro from AI: "Add comment '# init block' and print Starting"
macro from AI: "Insert comment '// data loading' then set load_done = true and print load_done"
macro from AI: "Add comment '# cleanup' and print Done"

# --- 9) Loop stress ---------------------------------------------------
neuro "--- LOOP STRESS TESTS ---"

macro from AI: Show Alert 4 times
macro from AI: Show Done 6 times
macro from AI: Show Status 8 times
macro from AI: "Say Ping 3 times"
macro from AI: Show car 1 time

# --- 10) Mixed cases --------------------------------------------------
neuro "--- MIXED TESTS ---"

set warnings = 2
macro from AI: "If warnings == 0 print All clear, else print Investigate warnings"

set errors = 0
macro from AI: "If errors == 0 print No errors, else print Errors found"

set stage = "plan"
macro from AI: "If stage == plan print Planning, elif stage == build print Building, else print Launching"

set retries = 2
macro from AI: "Decrease retries by 1 and print retries"

macro from AI: "Set summary = 'Macro tests complete' and print summary"

neuro "=== MACRO TEST END ==="

