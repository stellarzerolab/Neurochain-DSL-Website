use std::env;
use std::fs;
use std::io::{self, Write};

use neurochain::engine::{analyze, analyze_blocks};
use neurochain::interpreter::Interpreter;
use neurochain::banner;

const NEUROCHAIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const NEUROCHAIN_ABOUT: &str =
    "NeuroChain CLI – built for AI, logic and elegance. StellarZeroLabs © 2026.";

fn print_version() {
    println!("🧬 NeuroChain version {}", NEUROCHAIN_VERSION);
}

fn print_about() {
    println!("🌌 {}", NEUROCHAIN_ABOUT);
}

fn print_help() {
    println!(
        r#"
NeuroChain language — help

Basic syntax:
────────────────────────────────
AI: "path/to/model.onnx"        → Select an ONNX model
macro from AI: ...               → MacroIntent (intent → deterministic DSL template)
neuro "text"                     → Print a string
set x = "value"                  → Set a variable
set x from AI: "input"           → Run the active model into a variable
neuro x                          → Print a variable

Macros (intent → DSL):
────────────────────────────────
AI: "models/intent_macro/model.onnx"
macro from AI: Show Ping 3 times
macro from AI: "If score >= 10 say Congrats else say Nope"

Tip: if your prompt contains DSL keywords (`if/elif/else/and/or`), wrap it in quotes.
Loop macros clamp repeat counts to `1..=12` to prevent output flooding.

Control flow:
────────────────────────────────
if x == "value":
    neuro "..."                 → Runs when true

elif x != "value":
    neuro "..."                 → Additional condition

else:
    neuro "..."                 → Fallback branch

Logical operators:
────────────────────────────────
and, or                        → Example: if a == "X" and b != "Y":

Arithmetic:
────────────────────────────────
+  -  *  /  %                 → Example: set x = "4" + "2"
                               → To concat text + number: "" + number

Comparison operators:
────────────────────────────────
==  !=  <  >  <=  >=          → Example: if "3" > "1":
                               → Comparisons are case-insensitive

Variable expressions:
────────────────────────────────
set a = "5"
set b = "3"
set sum = a + b

Comments:
────────────────────────────────
# Comment                      → Ignored
// Comment                     → Also supported

Variables:
────────────────────────────────
If `neuro var` is not found in variables, the input is treated as a literal (fallback).

Supported AI models:
────────────────────────────────
SST2 (Sentiment): "Positive" / "Negative"
   set mood from AI: "This is amazing!"
   if mood == "Positive":
       neuro "Great"

Toxicity: "Toxic" / "Not toxic"
   set tox from AI: "You are bad."
   if tox == "Toxic":
       neuro "Warning"

FactCheck: "entailment" / "contradiction" / "neutral"
   set fact from AI: "Earth is flat. | Earth is round."
   if fact == "contradiction":
       neuro "Contradiction detected"

Intent: e.g. "GoCommand", "StopCommand", "LeftCommand"
   set cmd from AI: "Please stop."
   if cmd == "StopCommand":
       neuro "Stopping process"

MacroIntent: Loop/Branch/Arith/Concat/RoleFlag/AIBridge/DocPrint/SetVar/Unknown
   AI: "models/intent_macro/model.onnx"
   macro from AI: Show Ping 3 times
   macro from AI: "If score >= 10 say Congrats else say Nope"

Run commands (CLI & server):
────────────────────────────────
# CLI (interpreter)
cargo run --bin neurochain
cargo run --release --bin neurochain -- examples/macro_test.nc

# REST API server
cargo run --bin neurochain-server
cargo run --release --bin neurochain-server

Optional logging:
────────────────────────────────
NEUROCHAIN_OUTPUT_LOG=1       → write `neuro:` output to a file (logs/run_latest.log)
NEUROCHAIN_RAW_LOG=1          → write intent/DSL debug to a file (logs/macro_raw_latest.log)

Docs & examples: https://github.com/stellarzerolabs/neurochain
"#
    );
}

fn main() {
    banner::print_banner();
    let mut interpreter = Interpreter::new();

    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let arg = &args[1];
        match arg.as_str() {
            "help" | "--help" | "-h" => {
                print_help();
                return;
            }
            "--version" | "-v" => {
                print_version();
                return;
            }
            "--about" => {
                print_about();
                return;
            }
            _ => {
                match fs::read_to_string(arg) {
                    Ok(contents) => {
                        println!("Running script: {arg}");
                        match analyze_blocks(&contents, &mut interpreter) {
                            Ok(_) => println!("Script finished."),
                            Err(err) => eprintln!("Error: {err}"),
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading file: {e}");
                    }
                }
                return;
            }
        }
    }

    // Interactive mode
    loop {
        println!("Enter NeuroChain code (finish with an empty line):");

        let mut input_block = String::new();
        loop {
            print!("... ");
            io::stdout().flush().unwrap();

            let mut line = String::new();
            io::stdin().read_line(&mut line).unwrap();

            if line.trim().is_empty() {
                break;
            }

            input_block.push_str(&line);
        }

        let trimmed = input_block.trim();
        match trimmed {
            "exit" => {
                println!("Exiting...");
                break;
            }
            "help" => {
                print_help();
                continue;
            }
            "version" | "--version" | "-v" => {
                print_version();
                continue;
            }
            "about" | "--about" => {
                print_about();
                continue;
            }
            "" => continue,
            _ => {}
        }

        match analyze(trimmed, &mut interpreter) {
            Ok(_) => {}
            Err(err) => eprintln!("Error: {err}"),
        }
    }
}
