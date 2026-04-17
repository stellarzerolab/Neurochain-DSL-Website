use std::env;
use std::fs;
use std::io::{self, Read};

use neurochain::actions::parse_action_plan_from_txrep;
use neurochain::banner;

fn print_usage() {
    eprintln!("Usage: txrep-to-action <txrep.json|->");
    eprintln!("Reads Stellar txrep/tx decode JSON and outputs ActionPlan JSON.");
}

fn read_input(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|err| format!("failed to read stdin: {err}"))?;
        return Ok(buf);
    }
    fs::read_to_string(path).map_err(|err| format!("failed to read {path}: {err}"))
}

fn main() {
    banner::print_banner_stderr();
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        print_usage();
        std::process::exit(2);
    };

    let input = match read_input(&path) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    };

    let plan = match parse_action_plan_from_txrep(&input) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    };

    let json = serde_json::to_string_pretty(&plan).unwrap_or_else(|err| {
        eprintln!("Error: failed to serialize ActionPlan: {err}");
        std::process::exit(1);
    });

    println!("{json}");
}
