AI: "models/intent_macro/model.onnx"

neuro "=== MACRO TEST SEMANTICS START ==="

# This test focuses on semantics, not just “does it crash”:
# - macro sets a variable → the value is used later
# - arith → branch (computed value drives a condition)
# - concat → store into a variable → use later
# - docprint/comment → set + later branch
# - multi-elif chains (indent/dedent)

# --- 1) SetVar -> use -------------------------------------------------
neuro "--- SETVAR -> USE ---"

# EXPECT: 5
macro from AI: "Set x to 5"
macro from AI: "Print the value of x"

# --- 2) Arith -> Branch -----------------------------------------------
neuro "--- ARITH -> BRANCH ---"

set a = 1
set b = 2

# EXPECT: 6
macro from AI: "Calculate (a + b) * 2 and store in r"
macro from AI: "Print the value of r"

# EXPECT: Six
macro from AI: "If r equals 6 say Six else say Other"

set x = 10
set y = 2

# EXPECT: 2
macro from AI: "Subtract y from x, divide by 4, store in q"
macro from AI: "Print the value of q"

# EXPECT: QOK
macro from AI: "If q == 2 say QOK else say QFAIL"

# --- 3) Concat -> store -> use ----------------------------------------
neuro "--- CONCAT -> STORE -> USE ---"

set name = "Joe"
set score = 10

# EXPECT: Joe10
macro from AI: "Concatenate name and score with '+' and store in result"
macro from AI: "Print the value of result"

# EXPECT: Match
macro from AI: "If result equals Joe10 say Match else say No"

# EXPECT: Hello Joe
macro from AI: "Print 'Hello ' + name"

set greeting = "Hi"
set target = "Team"

# EXPECT: Hi Team
macro from AI: "Print greeting + ' ' + target"

# --- 4) Comment + set -> Branch ---------------------------------------
neuro "--- COMMENT/SET -> BRANCH ---"

# NOTE: the comment may or may not be printed; the important part is setting the variable.
# EXPECT: true
macro from AI: "Insert comment // data loading then set load_done = true and print load_done"

# EXPECT: Loaded
macro from AI: "If load_done is true say Loaded else say NotLoaded"

# --- 5) Multi-elif chain ----------------------------------------------
neuro "--- MULTI ELIF ---"

set n = 3
# EXPECT: Three
macro from AI: "If n equals 1 say One, elif n equals 2 say Two, elif n equals 3 say Three, else say Other"

# --- 6) Arith assign -> later use -------------------------------------
neuro "--- ARITH ASSIGN -> USE ---"

# EXPECT: 2
macro from AI: "Set remainder = 17 % 5 and print remainder"

# EXPECT: RemainderOK
macro from AI: "If remainder == 2 say RemainderOK else say RemainderBad"

# --- 7) Loop determinism ----------------------------------------------
neuro "--- LOOP (DETERMINISTIC) ---"

# EXPECT: Ping (3 lines)
macro from AI: "Say Ping 3 times"

neuro "=== MACRO TEST SEMANTICS END ==="
