use std::env;
use std::fs;
use std::io::{self, Write};

use neurochain::banner;
use neurochain::engine::{analyze, analyze_blocks};
use neurochain::help_text::neurochain_language_help;
use neurochain::interpreter::Interpreter;

const NEUROCHAIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const NEUROCHAIN_ABOUT: &str =
    "NeuroChain CLI – built for AI, logic and elegance. StellarZeroLab © 2026.";

fn print_version() {
    println!("🧬 NeuroChain version {}", NEUROCHAIN_VERSION);
}

fn print_about() {
    println!("🌌 {}", NEUROCHAIN_ABOUT);
}

fn print_help() {
    println!("{}", neurochain_language_help());
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
