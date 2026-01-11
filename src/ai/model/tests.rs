use super::AIModel;
use anyhow::Result;
use std::path::Path;

fn should_skip(model_path: &str) -> bool {
    if Path::new(model_path).exists() {
        return false;
    }

    // These are smoke-tests for local development. The repo (or CI) may run without ONNX assets.
    eprintln!("skipping AI model test; missing file: {model_path}");
    true
}

#[test]
fn test_sst2_model_loading() -> Result<()> {
    let model_path = "models/distilbert-sst2/model.onnx";
    if should_skip(model_path) {
        return Ok(());
    }

    let model = AIModel::new(model_path)?;
    let result = model.predict("This is wonderful!")?;
    println!("SST2-tulos: {}", result);
    assert!(result == "Positive" || result == "Negative");
    Ok(())
}

#[test]
fn test_toxic_model_loading() -> Result<()> {
    let model_path = "models/toxic_quantized/model.onnx";
    if should_skip(model_path) {
        return Ok(());
    }

    let model = AIModel::new(model_path)?;
    let result = model.predict("You suck!")?;
    println!("Toxic-tulos: {}", result);
    assert!(result == "Toxic" || result == "Not toxic");
    Ok(())
}

#[test]
fn test_factcheck_model_loading() -> Result<()> {
    let model_path = "models/factcheck/model.onnx";
    if should_skip(model_path) {
        return Ok(());
    }

    let model = AIModel::new(model_path)?;
    let result = model.predict("The sun is hot.")?;
    println!("FactCheck-tulos: {}", result);
    assert!(["entailment", "neutral", "contradiction"].contains(&result.as_str()));
    Ok(())
}

#[test]
fn test_intent_model_loading() -> Result<()> {
    let model_path = "models/intent/model.onnx";
    if should_skip(model_path) {
        return Ok(());
    }

    let model = AIModel::new(model_path)?;
    let result = model.predict("Go right")?;
    println!("Intent-tulos: {}", result);
    assert!(result.ends_with("Command") || result == "OtherCommand");
    Ok(())
}
