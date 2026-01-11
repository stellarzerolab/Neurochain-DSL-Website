//! AI model loader + classifier (CPU ONNX).

use std::{path::Path, rc::Rc};

use anyhow::{anyhow, Result};
use tokenizers::{
    PaddingDirection, PaddingParams, Tokenizer, TruncationDirection, TruncationParams,
    TruncationStrategy,
};
use tract_ndarray::prelude::{Array as TractArray, Ix2 as TractIx2, IxDyn as TractIxDyn};
use tract_onnx::prelude::*;

type TractPlan = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

/* -------------------------------------------------------------------------- */
#[derive(Clone, Debug, PartialEq)]
pub enum ModelKind {
    SST2,
    Toxic,
    FactCheck,
    Intent,
    MacroIntent,
    Unknown,
}

#[derive(Clone)]
pub struct AIModel {
    plan: Rc<TractPlan>,
    tokenizer: Tokenizer,
    model_kind: ModelKind,
    pad_token: String,
}

/* ========================================================================== */
impl AIModel {
    /* ---- loader ------------------------------------------------------- */
    pub fn new(model_path: &str) -> Result<Self> {
        if !Path::new(model_path).exists() {
            return Err(anyhow!("Model file not found: {model_path}"));
        }

        /* Model type (heuristic from file path) */
        let model_kind = if model_path.contains("intent_macro") {
            ModelKind::MacroIntent
        } else if model_path.contains("sst2") {
            ModelKind::SST2
        } else if model_path.contains("toxic") {
            ModelKind::Toxic
        } else if model_path.contains("factcheck") {
            ModelKind::FactCheck
        } else if model_path.contains("intent") {
            ModelKind::Intent
        } else {
            ModelKind::Unknown
        };

        /* Tokenizer path = same directory as model.onnx */
        let tok_path = Path::new(model_path)
            .parent()
            .ok_or_else(|| anyhow!("Tokenizer directory missing"))?
            .join("tokenizer.json");
        let (tokenizer, pad_token) = Self::prepare_tokenizer(&tok_path, &model_kind)?;

        let plan = tract_onnx::onnx()
            .model_for_path(model_path)?
            .into_optimized()?
            .into_runnable()?;

        Ok(Self {
            plan: Rc::new(plan),
            tokenizer,
            model_kind,
            pad_token,
        })
    }
    /* ---- inference ---------------------------------------------------- */
    pub fn predict(&self, text: &str) -> Result<String> {
        let (label, _) = self.predict_with_score(text)?;
        Ok(label)
    }

    pub fn kind(&self) -> ModelKind {
        self.model_kind.clone()
    }

    /// Returns (label, softmax score)
    pub fn predict_with_score(&self, text: &str) -> Result<(String, f32)> {
        let mut enc = self.tokenizer.encode(text, true).map_err(|e| anyhow!(e))?;
        enc.pad(128, 0, 0, self.pad_token.as_str(), PaddingDirection::Left);
        enc.truncate(128, 0, TruncationDirection::Right);

        let ids = TractArray::from_shape_vec(
            TractIxDyn(&[1, 128]),
            enc.get_ids().iter().map(|&id| id as i64).collect(),
        )?
        .into_tensor();
        let mask = TractArray::from_shape_vec(
            TractIxDyn(&[1, 128]),
            enc.get_attention_mask().iter().map(|&m| m as i64).collect(),
        )?
        .into_tensor();

        let outs = self.plan.run(tvec![ids.into(), mask.into()])?;
        let logits = outs[0]
            .to_array_view::<f32>()?
            .into_dimensionality::<TractIx2>()?;
        let row = logits.row(0);

        let labels: &[&str] = match self.model_kind {
            ModelKind::SST2 => &["Negative", "Positive"],
            ModelKind::Toxic => &["Toxic", "Not toxic"],
            ModelKind::FactCheck => &["entailment", "neutral", "contradiction"],
            ModelKind::Intent => &[
                "RightCommand",
                "LeftCommand",
                "UpCommand",
                "DownCommand",
                "GoCommand",
                "StopCommand",
                "OtherCommand",
            ],
            ModelKind::MacroIntent => &[
                "Loop", "Branch", "Arith", "Concat", "RoleFlag", "AIBridge", "DocPrint", "SetVar",
                "Unknown",
            ],
            ModelKind::Unknown => &["unknown"],
        };
        let (best_idx, prob) = argmax_with_prob(row.iter().copied());
        let label = labels
            .get(best_idx)
            .copied()
            .unwrap_or("unknown")
            .to_string();

        Ok((label, prob))
    }

    /* ---- tokenizer helper -------------------------------------------- */
    fn prepare_tokenizer(path: &Path, _kind: &ModelKind) -> Result<(Tokenizer, String)> {
        let mut tok = Tokenizer::from_file(path).map_err(|e| anyhow!(e))?;

        let candidates = ["[PAD]", "<pad>", "<PAD>", "PAD"];
        let pad_token = candidates
            .iter()
            .find(|name| tok.token_to_id(name).is_some())
            .ok_or_else(|| anyhow!("Tokenizer is missing a known pad token"))?
            .to_string();

        let pad_id = tok
            .token_to_id(pad_token.as_str())
            .ok_or_else(|| anyhow!("Pad token \"{}\" not found in tokenizer", pad_token))?;

        tok.with_padding(Some(PaddingParams {
            strategy: tokenizers::PaddingStrategy::Fixed(128),
            direction: PaddingDirection::Left,
            pad_to_multiple_of: None,
            pad_id,
            pad_type_id: 0,
            pad_token: pad_token.clone(),
        }));
        let _ = tok.with_truncation(Some(TruncationParams {
            max_length: 128,
            strategy: TruncationStrategy::LongestFirst,
            stride: 0,
            direction: TruncationDirection::Right,
        }));

        Ok((tok, pad_token))
    }
}

/* -------------------------------------------------------------------------- */
fn argmax_with_prob<I>(iter: I) -> (usize, f32)
where
    I: IntoIterator<Item = f32>,
{
    let vals: Vec<f32> = iter.into_iter().collect();
    let mut best_idx = 0;
    let mut best_val = f32::MIN;
    for (i, v) in vals.iter().enumerate() {
        if v > &best_val {
            best_idx = i;
            best_val = *v;
        }
    }
    let exp_sum: f32 = vals.iter().map(|v| (*v - best_val).exp()).sum();
    let prob = if exp_sum > 0.0 { 1.0 / exp_sum } else { 0.0 };
    (best_idx, prob)
}

#[cfg(test)]
mod tests;
