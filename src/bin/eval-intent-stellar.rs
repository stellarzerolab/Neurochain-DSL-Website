use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

use anyhow::{anyhow, Context, Result};
use neurochain::ai::model::AIModel;
use serde::Deserialize;

const LABELS: [&str; 9] = [
    "BalanceQuery",
    "CreateAccount",
    "ChangeTrust",
    "TransferXLM",
    "TransferAsset",
    "FundTestnet",
    "TxStatus",
    "ContractInvoke",
    "Unknown",
];

#[derive(Debug, Deserialize)]
struct DatasetRow {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    instruction: Option<String>,
    intent: String,
}

#[derive(Debug)]
struct Miss {
    expected: String,
    predicted: String,
    score: f32,
    text: String,
}

fn print_help() {
    println!(
        "Usage:
  cargo run --bin eval-intent-stellar -- [--input <jsonl>] [--model <onnx>] [--show-misses <n>]

Defaults:
  --input       datasets/test.jsonl
  --model       models/intent_stellar/model.onnx
  --show-misses 20"
    );
}

fn parse_args() -> Result<(String, String, usize)> {
    let mut input = "datasets/test.jsonl".to_string();
    let mut model = "models/intent_stellar/model.onnx".to_string();
    let mut show_misses = 20usize;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--input" => {
                i += 1;
                input = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow!("missing --input value"))?;
            }
            "--model" => {
                i += 1;
                model = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| anyhow!("missing --model value"))?;
            }
            "--show-misses" => {
                i += 1;
                let raw = args
                    .get(i)
                    .ok_or_else(|| anyhow!("missing --show-misses value"))?;
                show_misses = raw
                    .parse::<usize>()
                    .with_context(|| format!("invalid --show-misses value: {raw}"))?;
            }
            other => return Err(anyhow!("unknown arg: {other}")),
        }
        i += 1;
    }

    Ok((input, model, show_misses))
}

fn safe_div(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        0.0
    } else {
        a / b
    }
}

fn f1(p: f64, r: f64) -> f64 {
    if p + r == 0.0 {
        0.0
    } else {
        2.0 * p * r / (p + r)
    }
}

fn main() -> Result<()> {
    let (input_path, model_path, show_misses) = parse_args()?;

    let model = AIModel::new(&model_path)
        .with_context(|| format!("failed to load model at {}", model_path))?;

    let fh = File::open(&input_path).with_context(|| format!("failed to open {}", input_path))?;
    let rd = BufReader::new(fh);

    let mut total = 0usize;
    let mut correct = 0usize;
    let mut skipped = 0usize;

    let mut support: HashMap<String, usize> = HashMap::new();
    let mut confusion: HashMap<(String, String), usize> = HashMap::new();
    let mut misses: Vec<Miss> = Vec::new();

    for line in rd.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let row: DatasetRow = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let text = row
            .instruction
            .clone()
            .or(row.text.clone())
            .unwrap_or_default();
        if text.is_empty() || row.intent.is_empty() {
            skipped += 1;
            continue;
        }

        if !LABELS.contains(&row.intent.as_str()) {
            skipped += 1;
            continue;
        }

        total += 1;
        *support.entry(row.intent.clone()).or_insert(0) += 1;

        let (pred, score) = model.predict_with_score(&text)?;
        *confusion
            .entry((row.intent.clone(), pred.clone()))
            .or_insert(0) += 1;
        if pred == row.intent {
            correct += 1;
        } else {
            misses.push(Miss {
                expected: row.intent,
                predicted: pred,
                score,
                text,
            });
        }
    }

    if total == 0 {
        return Err(anyhow!("no valid samples found in {}", input_path));
    }

    let acc = safe_div(correct as f64, total as f64);

    println!("Model:  {}", model_path);
    println!("Input:  {}", input_path);
    println!("Rows:   {} (skipped {})", total, skipped);
    println!("Acc:    {:.4} ({}/{})", acc, correct, total);
    println!();
    println!(
        "{:16} {:>7} {:>8} {:>8} {:>8}",
        "Label", "Support", "Prec", "Recall", "F1"
    );
    println!("{}", "-".repeat(52));

    let mut macro_f1_sum = 0.0f64;
    let mut label_count = 0usize;

    for label in LABELS {
        let tp = *confusion
            .get(&(label.to_string(), label.to_string()))
            .unwrap_or(&0) as f64;

        let mut fp = 0f64;
        let mut fn_ = 0f64;
        for other in LABELS {
            if other == label {
                continue;
            }
            fp += *confusion
                .get(&(other.to_string(), label.to_string()))
                .unwrap_or(&0) as f64;
            fn_ += *confusion
                .get(&(label.to_string(), other.to_string()))
                .unwrap_or(&0) as f64;
        }

        let p = safe_div(tp, tp + fp);
        let r = safe_div(tp, tp + fn_);
        let f = f1(p, r);
        let sup = *support.get(label).unwrap_or(&0);

        if sup > 0 {
            macro_f1_sum += f;
            label_count += 1;
        }

        println!("{:16} {:>7} {:>8.4} {:>8.4} {:>8.4}", label, sup, p, r, f);
    }

    let macro_f1 = safe_div(macro_f1_sum, label_count as f64);
    println!();
    println!("MacroF1 (labels with support): {:.4}", macro_f1);

    if !misses.is_empty() && show_misses > 0 {
        misses.sort_by(|a, b| a.score.total_cmp(&b.score));
        println!();
        println!("Misses (up to {}):", show_misses);
        for m in misses.iter().take(show_misses) {
            println!(
                "- expected={} predicted={} score={:.4} text=\"{}\"",
                m.expected, m.predicted, m.score, m.text
            );
        }
    }

    Ok(())
}
