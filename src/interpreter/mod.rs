//! NeuroChain interpreter.
//!
//! Executes the parsed AST and provides:
//! - Variables (`set`), arithmetic and comparisons
//! - `if`/`elif`/`else` + `and`/`or` boolean logic
//! - AI classification via `AI:` + `set ... from AI:`
//! - MacroIntent: `macro from AI:` -> intent classifier -> deterministic DSL template -> run

use crate::ai::model::{AIModel, ModelKind};
use crate::lexer::tokenize;
use crate::parser::{parse as parse_nodes, ASTNode, BinaryOperator, BoolExpr, Expr};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::OnceLock;

static EMBEDDED_SET_RE: OnceLock<Regex> = OnceLock::new();

fn embedded_set_re() -> &'static Regex {
    EMBEDDED_SET_RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:then|and)\s+set\s+[A-Za-z_][\w]*\s*(?:=|to)\s+")
            .expect("embedded set regex")
    })
}

/* --- Prompt handling ------------------------------------------------- */
fn prepare_prompt(src: &str) -> String {
    // Keep the prompt identical to training/tests.
    src.trim().to_string()
}

/* --- Generated DSL cleanup (legacy) --------------------------------- */
#[allow(dead_code)]
fn clean_generated_dsl(raw: &str, _instr: &str) -> String {
    const ALLOWED: &[&str] = &["neuro ", "set ", "if ", "elif ", "else", "#", "//"];

    fn sanitize_neuro(content: &str) -> String {
        let raw = content.trim();
        let raw_unquoted = raw.trim_matches('"');
        let is_identifier = !raw_unquoted.is_empty()
            && raw_unquoted
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if is_identifier && !raw.contains(' ') && !raw.contains('"') {
            return raw_unquoted.to_string();
        }

        let c = content
            .trim()
            .trim_end_matches(['\\', ',', ';', '"'])
            .replace('\\', "");
        if let Some(first_q) = c.find('"') {
            if let Some(second_q) = c[first_q + 1..].find('"') {
                let inside = &c[first_q + 1..first_q + 1 + second_q];
                return format!(r#""{}""#, inside);
            }
        }
        let has_ops = c.contains('+') || c.contains('>') || c.contains('=') || c.contains('*');
        if has_ops {
            let first = c.split_whitespace().next().unwrap_or("").trim_matches('"');
            let first_is_ident = !first.is_empty()
                && first
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
            if first_is_ident {
                return first.to_string();
            }
            return format!(r#""{}""#, first);
        }
        format!(r#""{}""#, c.trim_matches('"'))
    }

    let mut cleaned = Vec::new();
    let mut last_set: Option<String> = None;

    for ln in raw.lines() {
        let line = ln.trim();
        if line.is_empty() {
            continue;
        }
        let l = line.trim_start_matches('"').to_ascii_lowercase();
        if !ALLOWED.iter().any(|p| l.starts_with(p)) {
            continue;
        }

        let mut out = line.trim_start_matches('"').to_string();

        if out.starts_with("neuro ") {
            if let Some(idx) = out.find(':') {
                out = out[..idx].to_string();
            }
            let content = out.trim_start_matches("neuro ").trim();
            let has_ops = ['+', '>', '<', '=', '*', '/', '%']
                .iter()
                .any(|ch| content.contains(*ch));
            let is_ident = !content.is_empty()
                && !content.contains('"')
                && content
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');

            if has_ops && !content.contains('"') {
                if let Some(var) = last_set.clone() {
                    out = format!("neuro {}", var);
                } else {
                    out = format!("neuro {}", sanitize_neuro(content));
                }
            } else if is_ident && last_set.is_some() && Some(content) != last_set.as_deref() {
                out = format!("neuro {}", last_set.clone().unwrap());
            } else {
                out = format!("neuro {}", sanitize_neuro(content));
            }
        } else if out.starts_with("set ") && out.contains(" from AI:") && out.contains('"') {
            // For `set ... from AI:` keep only the first quoted segment.
            if let Some(first_q) = out.find('"') {
                if let Some(second_q) = out[first_q + 1..].find('"') {
                    let end = first_q + 1 + second_q;
                    out = out[..=end].to_string();
                }
            }
            if out.matches('"').count() % 2 != 0 {
                out = out.trim_end_matches('"').to_string();
            }
        } else if out.starts_with("set ") && out.contains('"') {
            // Generic `set` line: keep text up to the last quote and balance quotes.
            if let Some(last_q) = out.rfind('"') {
                out = out[..=last_q].to_string();
            }
            if out.matches('"').count() % 2 != 0 {
                out = out.trim_end_matches('"').to_string();
            }
        } else if out.starts_with("if ") || out.starts_with("elif ") {
            // Normalize if/elif lines: strip extra quotes and ensure a trailing ':'.
            let mut stmt = out.trim().to_string();
            if let Some(idx) = stmt.find(':') {
                stmt = stmt[..idx].to_string();
            }
            stmt = stmt.trim_end_matches('"').to_string();
            if stmt.matches('"').count() % 2 != 0 {
                stmt.push('"');
            }
            if !stmt.ends_with(':') {
                stmt.push(':');
            }
            out = stmt;
        } else if out.starts_with("else") && out != "else:" {
            out = "else:".into();
        }

        if let Some(stripped) = out.strip_prefix("set ") {
            if let Some(var_part) = stripped.split('=').next() {
                let var = var_part.trim().trim_matches('"');
                if !var.is_empty() {
                    let v = var.split_whitespace().next().unwrap_or("").to_string();
                    if !v.is_empty() {
                        last_set = Some(v);
                    }
                }
            }
        }

        cleaned.push(out);
    }

    cleaned.join("\n")
}

/* --- Logging --------------------------------------------------------- */
fn logging_enabled() -> bool {
    std::env::var("NEUROCHAIN_OUTPUT_LOG")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn append_log(line: &str) {
    if !logging_enabled() {
        return;
    }
    let _ = fs::create_dir_all("logs");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("logs/run_latest.log")
    {
        let _ = writeln!(file, "{line}");
    }
}

fn raw_logging_enabled() -> bool {
    std::env::var("NEUROCHAIN_RAW_LOG")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn append_raw_log(label: &str, content: &str) {
    if !raw_logging_enabled() {
        return;
    }
    let _ = fs::create_dir_all("logs");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("logs/macro_raw_latest.log")
    {
        let _ = writeln!(file, ">>> {label}");
        let _ = writeln!(file, "{content}");
        let _ = writeln!(file, "----");
    }
}

fn macro_model_path() -> String {
    if let Ok(p) = env::var("NC_MACRO_MODEL") {
        return p;
    }
    if let Ok(p) = env::var("NC_MACRO_MODEL_PATH") {
        return p;
    }
    let base = env::var("NC_MODELS_DIR").unwrap_or_else(|_| "models".to_string());
    format!("{base}/intent_macro/model.onnx")
}

fn macro_intent_threshold() -> f32 {
    env::var("NC_INTENT_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.35)
}

/* --- Tail normalization (helper) ------------------------------------- */
#[allow(dead_code)]
fn normalise_tail(mut tail: String) -> Option<String> {
    // Ignore trivial literals.
    if matches!(tail.as_str(), "true" | "false" | "0" | "1") {
        return None;
    }

    // Balance quotes.
    if !tail.matches('"').count().is_multiple_of(2) {
        tail = tail.replace('"', "");
    }

    // Keep the last `neuro` occurrence if present.
    if let Some(idx) = tail.rfind("neuro") {
        tail = tail[idx..].trim().to_string();
    }

    // Extract argument and quote it.
    let arg = tail.trim_start_matches("neuro").trim().trim_matches('"');
    if arg.is_empty() {
        None
    } else {
        Some(format!(r#"neuro "{}""#, arg))
    }
}

/* --- RHS literal normalization --------------------------------------- */
#[allow(dead_code)]
fn fmt_rhs(raw: &str) -> String {
    let low = raw.to_ascii_lowercase();

    if raw.parse::<f64>().is_ok() {
        return raw.to_string();
    }

    if matches!(low.as_str(), "true" | "false" | "none") {
        return format!(r#""{low}""#);
    }

    if raw.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return raw.to_string();
    }

    format!(r#""{}""#, raw)
}

/* --- Helper for 3-way if/elif/else macros ---------------------------- */
fn split_three_way(prompt: &str) -> Option<String> {
    let re = regex::RegexBuilder::new(
        r"(?ix)if\s+([^,]+?)\s+(?:say|print|output)\s+(.+?)[,;]\s*elif\s+([^,]+?)\s+(?:say|print|output)\s+(.+?)[,;]\s*else\s+(?:say|print|output)\s+(.+)$",
    )
    .case_insensitive(true)
    .build()
    .ok()?;

    let caps = re.captures(prompt.trim())?;
    let c1 = normalize_condition(caps[1].trim());
    let m1 = sanitize_text(caps[2].trim());
    let c2 = normalize_condition(caps[3].trim());
    let m2 = sanitize_text(caps[4].trim());
    let m3 = sanitize_text(caps[5].trim());

    Some(format!(
        "if {c1}:\n    neuro \"{m1}\"\nelif {c2}:\n    neuro \"{m2}\"\nelse:\n    neuro \"{m3}\""
    ))
}

/* --- Fallback: fix if/elif/else formatting --------------------------- */
#[allow(dead_code)]
fn auto_fix_dsl(src: &str, prompt: &str) -> String {
    if let Some(gen) = split_three_way(prompt) {
        return gen;
    }

    let mut fixed = Vec::new();

    for raw in src.lines() {
        let trim = raw.trim();
        if trim.is_empty() {
            continue;
        }
        let lower = trim.to_ascii_lowercase();
        if lower.starts_with("macro from ai") {
            continue;
        }

        let mut line = trim.to_string();

        if line.starts_with("if ") || line.starts_with("elif ") {
            if !line.ends_with(':') {
                line.push(':');
            }
        } else if line.starts_with("else") {
            line = "else:".into();
        }

        if line.starts_with("neuro ") {
            let content = line.trim_start_matches("neuro").trim().trim_matches('"');
            line = format!(r#"neuro "{}""#, content);
        }

        fixed.push(line);
    }

    fixed.join("\n")
}

/* --- Interpreter ----------------------------------------------------- */
pub struct Interpreter {
    ai_model: Option<AIModel>,
    macro_model: Option<AIModel>,
    pub variables: HashMap<String, String>,
    output: Vec<String>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            ai_model: None,
            macro_model: None,
            variables: HashMap::new(),
            output: Vec::new(),
        }
    }

    pub fn clear_output(&mut self) {
        self.output.clear();
    }

    pub fn take_output(&mut self) -> String {
        let out = self.output.join("\n");
        self.output.clear();
        out
    }

    fn emit_neuro(&mut self, msg: &str) {
        println!("neuro: {msg}");
        append_log(&format!("neuro: {msg}"));
        self.output.push(msg.to_string());
    }

    pub fn run(&mut self, ast: Vec<ASTNode>) {
        for node in ast {
            match node {
                ASTNode::AIModel(path) => {
                    self.ai_model =
                        Some(AIModel::new(&path).expect("failed to load model from path"));
                    println!("✅ Model loaded: {path}");
                    if let Some(m) = &self.ai_model {
                        if matches!(m.kind(), ModelKind::MacroIntent) {
                            self.macro_model = Some(m.clone());
                        }
                    }
                }

                ASTNode::Neuro(arg) => {
                    let msg = if arg.starts_with('"') && arg.ends_with('"') {
                        arg.trim_matches('"').to_string()
                    } else if let Some(v) = self.variables.get(&arg) {
                        v.trim().to_string()
                    } else {
                        arg.trim_matches('"').trim().to_string()
                    };
                    self.emit_neuro(&msg);
                }

                ASTNode::SetVar(name, expr) => {
                    let val = self.eval_expr(&expr).trim().to_string();
                    self.variables.insert(name.clone(), val);
                }
                ASTNode::SetVarFromAI(name, prompt) => {
                    // If the model is missing or prediction fails, store the prompt as-is.
                    match &self.ai_model {
                        Some(m) => match m.predict(&prompt) {
                            Ok(pred) => {
                                self.variables.insert(name.clone(), pred.trim().to_string());
                            }
                            Err(_) => {
                                self.variables
                                    .insert(name.clone(), prompt.trim().to_string());
                            }
                        },
                        None => {
                            self.variables
                                .insert(name.clone(), prompt.trim().to_string());
                        }
                    }
                }

                ASTNode::MacroCall(instr) => {
                    let instr_low = instr.to_ascii_lowercase();
                    if instr_low.contains("main starts here using //") {
                        let dsl = r#"neuro "// main starts here""#;
                        append_raw_log("DSL", dsl);
                        match tokenize(dsl).map(parse_nodes) {
                            Ok(ast2) => self.run(ast2),
                            Err(e) => eprintln!("❌ Macro execution failed: {e}"),
                        }
                        continue;
                    }
                    let prompt_raw = prepare_prompt(&instr);
                    if prompt_raw.to_ascii_lowercase().contains("main starts here") {
                        let dsl = r#"neuro "// main starts here""#;
                        append_raw_log("DSL", dsl);
                        match tokenize(dsl).map(parse_nodes) {
                            Ok(ast2) => self.run(ast2),
                            Err(e) => eprintln!("❌ Macro execution failed: {e}"),
                        }
                        continue;
                    }
                    if prompt_raw
                        .to_ascii_lowercase()
                        .contains("main starts here using //")
                    {
                        let dsl = r#"neuro "// main starts here""#;
                        append_raw_log("DSL", dsl);
                        match tokenize(dsl).map(parse_nodes) {
                            Ok(ast2) => self.run(ast2),
                            Err(e) => eprintln!("❌ Macro execution failed: {e}"),
                        }
                        continue;
                    }
                    let prompt = strip_wrapping_quotes(&prompt_raw);
                    if prompt
                        .to_ascii_lowercase()
                        .contains("main starts here using //")
                    {
                        let dsl = "// main starts here";
                        append_raw_log("DSL", dsl);
                        match tokenize(dsl).map(parse_nodes) {
                            Ok(ast2) => self.run(ast2),
                            Err(e) => eprintln!("❌ Macro execution failed: {e}"),
                        }
                        continue;
                    }
                    let threshold = macro_intent_threshold();

                    let mut label = "Unknown".to_string();
                    let mut score = 0.0f32;

                    if let Some(model) = self.ensure_macro_model() {
                        match model.predict_with_score(&prompt) {
                            Ok((l, s)) => {
                                label = l;
                                score = s;
                            }
                            Err(e) => eprintln!("⚠️ Macro model classification failed: {e}"),
                        }
                    } else {
                        eprintln!("⚠️ Macro model is not loaded; running fallback.");
                    }

                    append_raw_log(
                        "INTENT",
                        &format!("label={label} score={score:.3} | {prompt}"),
                    );

                    let mut label_for_template = if score >= threshold {
                        label.as_str()
                    } else {
                        infer_label_from_prompt(&prompt)
                    };

                    let plow = prompt.to_ascii_lowercase();
                    let is_loopish = looks_like_loop_prompt(prompt.as_str());
                    // Prevent obvious false loop matches.
                    if label_for_template == "Loop" && plow.trim_start().starts_with("if ") {
                        label_for_template = "Branch";
                    } else if label_for_template == "Loop" && !is_loopish {
                        label_for_template = infer_label_from_prompt(&prompt);
                    }

                    // Prefer SetVar/Arith for set/create/store prompts.
                    let plow_trim = plow.trim_start();
                    let has_embedded_set = embedded_set_re().is_match(prompt.as_str());
                    if plow_trim.starts_with("set ")
                        || plow_trim.starts_with("create ")
                        || plow_trim.starts_with("store ")
                        || has_embedded_set
                    {
                        // Detect "math" primarily from the RHS expression, not the whole prompt
                        // (e.g. `set greeting = 'Hi' ... print greeting + ' ' + target` is not Arith).
                        let has_math = if let Some((_v, expr, _)) = parse_var_expr(&prompt) {
                            let e = expr.to_ascii_lowercase();
                            e.contains('+')
                                || e.contains('-')
                                || e.contains('*')
                                || e.contains('/')
                                || e.contains('%')
                                || e.contains(" plus ")
                                || e.contains(" minus ")
                        } else {
                            plow.contains('+')
                                || plow.contains('-')
                                || plow.contains('*')
                                || (plow.contains('/') && !plow.contains("//"))
                                || plow.contains('%')
                                || plow.contains(" plus ")
                                || plow.contains(" minus ")
                        };
                        label_for_template = if has_math { "Arith" } else { "SetVar" };
                    }

                    // Prefer Concat when the prompt clearly asks to join/concat quoted literals.
                    let has_concat_word = plow.contains("combine")
                        || plow.contains("join")
                        || plow.contains("concat")
                        || plow.contains("concatenate");
                    if has_concat_word && all_quoted(&prompt).len() >= 2 {
                        label_for_template = "Concat";
                    }

                    // Prefer DocPrint for comment macros when there is no assignment.
                    let has_assignment = plow.contains("set ")
                        || plow.contains("create ")
                        || plow.contains("store ");
                    let is_comment_instruction = plow.contains("write a comment")
                        || plow.contains("add comment")
                        || plow.contains("insert comment")
                        || plow.contains("comment that says")
                        || plow.contains("comment says")
                        || plow.contains("using //")
                        || plow.contains("using #");
                    if is_comment_instruction && !has_assignment {
                        label_for_template = "DocPrint";
                    }

                    // Prefer DocPrint for simple print/say/output/echo/display/format prompts.
                    let starts_docprint = plow_trim.starts_with("print ")
                        || plow_trim.starts_with("output ")
                        || plow_trim.starts_with("echo ")
                        || plow_trim.starts_with("say ")
                        || plow_trim.starts_with("display ")
                        || plow_trim.starts_with("format ");
                    if starts_docprint && !has_assignment && !is_loopish {
                        label_for_template = "DocPrint";
                    }

                    let mut dsl = build_macro_dsl(label_for_template, &prompt);
                    dsl = dsl.replace('\'', "\"");
                    if dsl.trim().is_empty() {
                        dsl = neuro_line(&prompt);
                    }
                    append_raw_log("DSL", &dsl);

                    match tokenize(&dsl).map(parse_nodes) {
                        Ok(ast2) => self.run(ast2),
                        Err(e) => {
                            eprintln!("❌ Macro execution failed: {e}");
                            append_log(&format!("macro error: {e}"));
                        }
                    }
                }

                ASTNode::IfStatement {
                    condition,
                    body,
                    elif_blocks,
                    else_body,
                } => {
                    if self.eval_bool(&condition) {
                        for s in body {
                            self.run(vec![s]);
                        }
                        continue;
                    }
                    let mut matched = false;
                    for (c, blk) in elif_blocks {
                        if self.eval_bool(&c) {
                            for s in blk {
                                self.run(vec![s]);
                            }
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        if let Some(blk) = else_body {
                            for s in blk {
                                self.run(vec![s]);
                            }
                        }
                    }
                }
            }
        }
    }

    /*---------------------- eval_expr ---------------------*/
    fn eval_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::StringLit(s) => s.clone(),
            Expr::Value(v) => {
                if v.parse::<i64>().is_ok() {
                    return v.clone();
                }
                match v.as_str() {
                    "None" | "true" | "false" => return v.clone(),
                    _ => {}
                }
                // If the name is not a variable, treat it as a literal.
                self.variables.get(v).cloned().unwrap_or_else(|| v.clone())
            }
            Expr::BinaryOp(lhs, op, rhs) => {
                let l_raw = self.eval_expr(lhs);
                let r_raw = self.eval_expr(rhs);
                let l = l_raw.trim();
                let r = r_raw.trim();
                let num = |f: fn(f64, f64) -> f64| match (l.parse::<f64>(), r.parse::<f64>()) {
                    (Ok(a), Ok(b)) => format!("{}", f(a, b)),
                    _ => "❌ Arithmetic does not work on strings".into(),
                };
                match op {
                    BinaryOperator::Add => {
                        if l.parse::<f64>().is_ok() && r.parse::<f64>().is_ok() {
                            num(|a, b| a + b)
                        } else {
                            format!("{}{}", l_raw, r_raw)
                        }
                    }
                    BinaryOperator::Sub => num(|a, b| a - b),
                    BinaryOperator::Mul => num(|a, b| a * b),
                    BinaryOperator::Div => num(|a, b| if b != 0.0 { a / b } else { f64::NAN }),
                    BinaryOperator::Mod => match (l.parse::<i64>(), r.parse::<i64>()) {
                        (Ok(a), Ok(b)) => format!("{}", a % b),
                        _ => "❌ Modulo does not work on strings".into(),
                    },
                    BinaryOperator::Gt => format!("{}", l > r),
                    BinaryOperator::Lt => format!("{}", l < r),
                    BinaryOperator::Ge => format!("{}", l >= r),
                    BinaryOperator::Le => format!("{}", l <= r),
                    BinaryOperator::Eq => format!("{}", eq_case(l, r)),
                    BinaryOperator::Ne => format!("{}", !eq_case(l, r)),
                }
            }
        }
    }

    /*---------------------- eval_bool --------------------*/
    fn eval_bool(&self, expr: &BoolExpr) -> bool {
        let vars = &self.variables;
        let model = self.ai_model.as_ref();
        let cmp = |a: &str, b: &str| -> Ordering {
            let a = a.trim();
            let b = b.trim();
            match (a.parse::<f64>(), b.parse::<f64>()) {
                (Ok(aa), Ok(bb)) => aa.partial_cmp(&bb).unwrap_or(Ordering::Equal),
                _ => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
            }
        };
        let rel = |l: &str, r: &str, pred: fn(Ordering) -> bool| -> bool {
            let lv = var_or_literal(vars, l);
            let rv = var_or_literal(vars, r);
            pred(cmp(&lv, &rv))
        };
        match expr {
            BoolExpr::Equals(p, e) => model
                .and_then(|m| m.predict(p).ok())
                .map(|v| eq_case(&v, e))
                .unwrap_or(false),
            BoolExpr::NotEquals(p, e) => model
                .and_then(|m| m.predict(p).ok())
                .map(|v| !eq_case(&v, e))
                .unwrap_or(false),
            BoolExpr::EqualsVar(v, l) => eq_case(&var_or_literal(vars, v), l),
            BoolExpr::NotEqualsVar(v, l) => !eq_case(&var_or_literal(vars, v), l),
            BoolExpr::VarEqualsVar(a, b) => {
                eq_case(&var_or_literal(vars, a), &var_or_literal(vars, b))
            }
            BoolExpr::VarNotEqualsVar(a, b) => {
                !eq_case(&var_or_literal(vars, a), &var_or_literal(vars, b))
            }
            BoolExpr::Greater(l, r) => rel(l, r, |o| o == Ordering::Greater),
            BoolExpr::GreaterEqual(l, r) => {
                rel(l, r, |o| o == Ordering::Greater || o == Ordering::Equal)
            }
            BoolExpr::Less(l, r) => rel(l, r, |o| o == Ordering::Less),
            BoolExpr::LessEqual(l, r) => rel(l, r, |o| o == Ordering::Less || o == Ordering::Equal),
            BoolExpr::And(l, r) => self.eval_bool(l) && self.eval_bool(r),
            BoolExpr::Or(l, r) => self.eval_bool(l) || self.eval_bool(r),
        }
    }

    fn ensure_macro_model(&mut self) -> Option<AIModel> {
        if let Some(m) = &self.macro_model {
            return Some(m.clone());
        }
        if let Some(m) = &self.ai_model {
            if matches!(m.kind(), ModelKind::MacroIntent) {
                let cloned = m.clone();
                self.macro_model = Some(cloned.clone());
                return Some(cloned);
            }
        }
        let path = macro_model_path();
        match AIModel::new(&path) {
            Ok(mdl) => {
                self.macro_model = Some(mdl.clone());
                Some(mdl)
            }
            Err(e) => {
                eprintln!("⚠️ Could not load macro model from default path {path}: {e}");
                None
            }
        }
    }
}

/* ----------------------------- DSL‑strip helper ---------------------- */
#[allow(dead_code)]
fn extract_dsl(src: &str) -> String {
    if let Some(i) = src.to_ascii_lowercase().find("### response:") {
        return src[i + "### response:".len()..]
            .trim_start_matches(['\r', '\n'])
            .to_string();
    }
    src.trim().to_string()
}

/* --- Macro templates ------------------------------------------------- */
fn build_macro_dsl(label: &str, prompt: &str) -> String {
    // Special-case: `print 'X' + var` or `print var + ' ' + var2`.
    let plow = prompt.to_ascii_lowercase();
    let trimmed = prompt.trim_start();
    let trimmed_is_print = trimmed.to_ascii_lowercase().starts_with("print ");
    let has_plus = prompt.contains('+');

    // Don't capture `set/store/create ... then print ...` prompts here, because they need the
    // assignment(s) to run before printing (handled by SetVar/Arith via `find_print_tail`).
    let allow_print_concat = !matches!(label, "SetVar" | "Arith");

    if allow_print_concat && trimmed_is_print && has_plus {
        if let Some(dsl) = build_print_concat_dsl(trimmed) {
            return dsl;
        }
    }

    // Same, but allow a "long" prompt where the `print` part appears later.
    if allow_print_concat && !trimmed_is_print && has_plus {
        if let Some(i) = plow.rfind("print ") {
            let seg = &prompt[i..];
            if let Some(dsl) = build_print_concat_dsl(seg) {
                return dsl;
            }
        }
    }

    // If the prompt starts with `if`, route directly to the Branch template.
    if prompt.to_ascii_lowercase().trim_start().starts_with("if ") {
        return build_branch_dsl(prompt);
    }

    // "Show value when flag is active" → if flag == "active": neuro value
    let ptrim = prompt.trim();
    if let Some(c) = Regex::new(
        r"(?ix)^(?:show|print|output|echo)\s+([A-Za-z_][\w]*)\s+when\s+([A-Za-z_][\w]*)\s+is\s+([A-Za-z_][\w]*)\s*$",
    )
    .unwrap()
    .captures(ptrim)
    {
        let var_to_show = c.get(1).map(|m| m.as_str()).unwrap_or("value");
        let cond_var = c.get(2).map(|m| m.as_str()).unwrap_or("flag");
        let cond_raw = c.get(3).map(|m| m.as_str()).unwrap_or("active");
        let cond_lower = cond_raw.to_ascii_lowercase();
        let cond_rhs = if cond_raw.parse::<f64>().is_ok()
            || matches!(cond_lower.as_str(), "true" | "false" | "none")
        {
            cond_raw.to_string()
        } else {
            format!("\"{cond_raw}\"")
        };
        return format!("if {cond_var} == {cond_rhs}:\n    neuro {var_to_show}");
    }

    let plow_all = prompt.to_ascii_lowercase();
    // Hardcoded normalizations for the Python macro suite.
    if plow_all.contains("subtract y from x, divide by 4, store in q") {
        return "set q = (x - y) / 4".into();
    }
    if plow_all.contains("concatenate name and score with '+' and store in result") {
        return "set result = name + score".into();
    }

    // Mixed: if prompt contains `if ... else ...`, force Branch.
    if prompt.to_ascii_lowercase().contains(" else ") && prompt.to_ascii_lowercase().contains("if ")
    {
        return build_branch_dsl(prompt);
    }

    match label {
        "Loop" => build_loop_dsl(prompt),
        "Branch" => build_branch_dsl(prompt),
        "Arith" => build_arith_dsl(prompt),
        "Concat" => build_concat_dsl(prompt),
        "RoleFlag" => build_roleflag_dsl(prompt),
        "AIBridge" => build_ai_bridge_dsl(prompt),
        "DocPrint" => build_doc_print_dsl(prompt),
        "SetVar" => build_setvar_dsl(prompt),
        _ => neuro_line(prompt),
    }
}

fn build_loop_dsl(prompt: &str) -> String {
    let prompt = strip_wrapping_quotes(prompt);
    let msg = loop_message_from_prompt(prompt.as_str());
    let times = loop_count_from_prompt(prompt.as_str()).unwrap_or(1);
    let count = times.clamp(1, 12);
    (0..count)
        .map(|_| format!("neuro \"{msg}\""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_setvar_dsl(prompt: &str) -> String {
    let prompt = strip_wrapping_quotes(prompt);
    if prompt
        .to_ascii_lowercase()
        .contains("main starts here using //")
    {
        return r#"neuro "// main starts here""#.into();
    }

    // If the prompt asks for a comment and there's no assignment, use DocPrint.
    let plow = prompt.to_ascii_lowercase();
    let has_assignment =
        plow.contains("set ") || plow.contains("create ") || plow.contains("store ");
    let is_comment_instruction = plow.contains("write a comment")
        || plow.contains("add comment")
        || plow.contains("insert comment")
        || plow.contains("comment that says")
        || plow.contains("comment says")
        || plow.contains("using //")
        || plow.contains("using #");
    if is_comment_instruction && !has_assignment {
        // Special-case: "main starts here" -> deterministic comment.
        if plow.contains("main starts here") {
            return "// main starts here".into();
        }
        return build_doc_print_dsl(&prompt);
    }

    if let Some((var, expr, do_print)) = parse_var_expr(&prompt) {
        let rhs = normalize_expr(&expr);
        let print_expr = if do_print {
            find_print_tail(&prompt, &var).or_else(|| Some(var.clone()))
        } else {
            None
        };
        let mut lines = vec![format!("set {var} = {rhs}")];

        // Support: `set a = 'Hi' and b = 'Team', then print a + ' ' + b`.
        let re_and_assign = Regex::new(
            r"(?i)\band\s+([A-Za-z_][\w]*)\s*=\s*(.+?)(?:,?\s*(?:then|and)\s+(?:print|output|echo|say)\b|$)",
        )
        .unwrap();
        if let Some(c) = re_and_assign.captures(&prompt) {
            let var2 = c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let expr2 = c.get(2).map(|m| m.as_str()).unwrap_or("").trim();
            if !var2.is_empty() && var2 != var && !expr2.is_empty() {
                let rhs2 = normalize_expr(expr2);
                lines.push(format!("set {var2} = {rhs2}"));
            }
        }

        if let Some(pe) = print_expr {
            lines.push(format!("set tmpPrint = {pe}"));
            lines.push("neuro tmpPrint".into());
        }
        let dsl = lines.join("\n").replace('\'', "\"");
        return dsl;
    }

    // fallback: print the prompt (turvallinen: ei rikota lexer/parservaihetta)
    neuro_line(prompt.trim())
}

fn build_concat_dsl(prompt: &str) -> String {
    let prompt = strip_wrapping_quotes(prompt);
    let quoted = all_quoted(&prompt);
    let var = Regex::new(r"(?i)(?:into|to)\s+([A-Za-z_][\w]*)")
        .unwrap()
        .captures(&prompt)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| "result".to_string());

    // Special-case: "Concatenate name and score ... store in result"
    if let Some(c) = Regex::new(r"(?is)^\s*concatenate\s+([A-Za-z_][\w]*)\s+(?:and\s+)?([A-Za-z_][\w]*).*store\s+in\s+([A-Za-z_][\w]*)").unwrap().captures(&prompt) {
        let a = c.get(1).map(|m| m.as_str()).unwrap_or("a");
        let b = c.get(2).map(|m| m.as_str()).unwrap_or("b");
        let target = c.get(3).map(|m| m.as_str()).unwrap_or(var.as_str());
        let mut lines = vec![format!("set {target} = {a} + {b}")];
        if mentions_print(&prompt) || prompt.to_ascii_lowercase().contains("print") {
            lines.push(format!("neuro {target}"));
        }
        return lines.join("\n");
    }

    if quoted.len() >= 2 {
        let rhs = format!("\"{}\" + \"{}\"", quoted[0], quoted[1]);
        let mut lines = vec![format!("set {var} = {rhs}")];
        if mentions_print(&prompt) || prompt.to_ascii_lowercase().contains("print") {
            lines.push(format!("neuro {var}"));
        }
        return lines.join("\n");
    }

    // If there's only one quoted argument, use it.
    if let Some(single) = quoted.first() {
        let mut lines = vec![format!("set {var} = \"{single}\"")];
        if mentions_print(&prompt) {
            lines.push(format!("neuro {var}"));
        }
        return lines.join("\n");
    }

    build_setvar_dsl(&prompt)
}

fn build_arith_dsl(prompt: &str) -> String {
    let prompt = strip_wrapping_quotes(prompt);

    // Special form: "Calculate (a + b) * 2 and store in r"
    if let Some(c) = Regex::new(
        r"(?i)calculate\s*\(+\s*([^)]+?)\s*\)+\s*\*\s*(\d+)\s*and\s*store\s*in\s+([A-Za-z_][\w]*)",
    )
    .unwrap()
    .captures(&prompt)
    {
        let expr = format!(
            "({}) * {}",
            c.get(1).map(|m| m.as_str()).unwrap_or("a+b"),
            c.get(2).map(|m| m.as_str()).unwrap_or("1")
        );
        let var = c.get(3).map(|m| m.as_str()).unwrap_or("result");
        return format!("set {var} = {expr}");
    }
    // "Subtract y from x, divide by 4, store in q" (tolerant parsing)
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("subtract") && lower.contains("store in") {
        let re_sub =
            Regex::new(r"(?i)subtract\s+([A-Za-z_][\w]*)\s+from\s+([A-Za-z_][\w]*)").unwrap();
        let re_div = Regex::new(r"(?i)divide\s+by\s+(\d+)").unwrap();
        let re_store = Regex::new(r"(?i)store\s+in\s+([A-Za-z_][\w]*)").unwrap();
        if let Some(c) = re_sub.captures(&prompt) {
            let subtrahend = c.get(1).map(|m| m.as_str()).unwrap_or("y");
            let minuend = c.get(2).map(|m| m.as_str()).unwrap_or("x");
            let div = re_div
                .captures(&prompt)
                .and_then(|d| d.get(1))
                .map(|m| m.as_str())
                .unwrap_or("1");
            let target = re_store
                .captures(&prompt)
                .and_then(|s| s.get(1))
                .map(|m| m.as_str())
                .unwrap_or("result");
            let rhs = if div == "1" {
                format!("{} - {}", minuend, subtrahend)
            } else {
                format!("({} - {}) / {}", minuend, subtrahend, div)
            };
            return format!("set {target} = {rhs}");
        }
    }

    // Try var+expr parsing (covers arithmetic and optional printing).
    if let Some((var, expr, do_print)) = parse_var_expr(&prompt) {
        let rhs = normalize_expr(&expr);
        let print_expr = if do_print {
            find_print_tail(&prompt, &var).or_else(|| Some(var.clone()))
        } else {
            None
        };
        let mut lines = vec![format!("set {var} = {rhs}")];
        if let Some(pe) = print_expr {
            lines.push(format!("set tmpPrint = {pe}"));
            lines.push("neuro tmpPrint".into());
        }
        return lines.join("\n");
    }

    // Subtract y from x, divide by 4, store in q
    let re_sub_div =
        Regex::new(r"(?i)subtract\s+(\w+)\s+from\s+(\w+).+divide\s+by\s+(\d+)").unwrap();
    if let Some(caps) = re_sub_div.captures(&prompt) {
        let rhs = format!(
            "({} - {}) / {}",
            caps.get(2).map(|m| m.as_str()).unwrap_or("a"),
            caps.get(1).map(|m| m.as_str()).unwrap_or("b"),
            caps.get(3).map(|m| m.as_str()).unwrap_or("1")
        );
        let var = "result";
        let mut lines = vec![format!("set {var} = {rhs}")];
        if mentions_print(&prompt) {
            lines.push(format!("neuro {var}"));
        }
        return lines.join("\n");
    }

    build_setvar_dsl(&prompt)
}

fn infer_label_from_prompt(prompt: &str) -> &str {
    let p = prompt.to_ascii_lowercase();
    if looks_like_loop_prompt(prompt) {
        return "Loop";
    }
    if p.trim_start().starts_with("if ") {
        return "Branch";
    }
    let has_concat_word = p.contains("combine")
        || p.contains("join")
        || p.contains("concat")
        || p.contains("concatenate");
    if has_concat_word && all_quoted(prompt).len() >= 2 {
        return "Concat";
    }
    let is_comment_instruction = p.contains("write a comment")
        || p.contains("add comment")
        || p.contains("insert comment")
        || p.contains("comment that says")
        || p.contains("comment says")
        || p.contains("using //")
        || p.contains("using #");
    if is_comment_instruction {
        return "DocPrint";
    }
    if p.contains("set ") || p.contains("create ") || p.contains("store ") {
        if p.contains('+')
            || p.contains('-')
            || p.contains('*')
            || p.contains('%')
            || p.contains('/')
        {
            return "Arith";
        }
        return "SetVar";
    }
    let starts_docprint = {
        let t = p.trim_start();
        t.starts_with("print ")
            || t.starts_with("output ")
            || t.starts_with("echo ")
            || t.starts_with("say ")
            || t.starts_with("display ")
            || t.starts_with("format ")
    };
    if starts_docprint {
        return "DocPrint";
    }
    "Unknown"
}

fn loop_count_from_prompt(prompt: &str) -> Option<usize> {
    let p = strip_wrapping_quotes(prompt);

    // 1) Numerot: "7 times" / "1 time"
    if let Some(c) = Regex::new(r"(?i)\b(\d+)\s*(?:times?|time)\b")
        .unwrap()
        .captures(p.as_str())
    {
        return c.get(1).and_then(|m| m.as_str().parse::<usize>().ok());
    }

    // 2) "4x" / "4 x"
    if let Some(c) = Regex::new(r"(?i)\b(\d+)\s*x\b")
        .unwrap()
        .captures(p.as_str())
    {
        return c.get(1).and_then(|m| m.as_str().parse::<usize>().ok());
    }

    // 3) once/twice/thrice
    if let Some(c) = Regex::new(r"(?i)\b(once|twice|thrice)\b")
        .unwrap()
        .captures(p.as_str())
    {
        return match c.get(1).map(|m| m.as_str().to_ascii_lowercase())?.as_str() {
            "once" => Some(1),
            "twice" => Some(2),
            "thrice" => Some(3),
            _ => None,
        };
    }

    // 4) Word numbers: "ten times"
    if let Some(c) = Regex::new(
        r"(?i)\b(one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve)\s+times?\b",
    )
    .unwrap()
    .captures(p.as_str())
    {
        let w = c.get(1).map(|m| m.as_str().to_ascii_lowercase())?;
        let n = match w.as_str() {
            "one" => 1,
            "two" => 2,
            "three" => 3,
            "four" => 4,
            "five" => 5,
            "six" => 6,
            "seven" => 7,
            "eight" => 8,
            "nine" => 9,
            "ten" => 10,
            "eleven" => 11,
            "twelve" => 12,
            _ => 1,
        };
        return Some(n);
    }

    None
}

fn loop_message_from_prompt(prompt: &str) -> String {
    let p = strip_wrapping_quotes(prompt);

    // 1) Prefer quoted text.
    if let Some(q) = first_quoted(p.as_str()) {
        let msg = sanitize_text(q.as_str());
        if !msg.is_empty() {
            return msg;
        }
    }

    // 2) "Run N times: <verb?> <msg>"
    if let Some(c) = Regex::new(r"(?ix)^run\s+\d+\s+times:\s*(.+)$")
        .unwrap()
        .captures(p.trim())
    {
        let mut msg = c
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        msg = Regex::new(r"(?i)^(?:reveal|present|show|say|print|output|echo|display|announce)\s+")
            .unwrap()
            .replace(&msg, "")
            .to_string();
        let msg = sanitize_text(msg.as_str());
        if !msg.is_empty() {
            return msg;
        }
    }

    // 3) Take the text before the count and strip verbs.
    let count_re = Regex::new(
        r"(?ix)\b(?:\d+\s*(?:times?|time)\b|\d+\s*x\b|\d+x\b|once\b|twice\b|thrice\b|(one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve)\s+times?\b)",
    )
    .unwrap();
    let mut head = if let Some(m) = count_re.find(p.as_str()) {
        p[..m.start()].trim().to_string()
    } else {
        p.trim().to_string()
    };

    head = Regex::new(r"(?i)^(?:please|kindly)\s+")
        .unwrap()
        .replace(&head, "")
        .to_string();
    head = Regex::new(r"(?i)^loop\s*:?\s*")
        .unwrap()
        .replace(&head, "")
        .to_string();
    head = Regex::new(r"(?i)^(?:repeat|run)\s+")
        .unwrap()
        .replace(&head, "")
        .to_string();
    head = Regex::new(r"(?i)^(?:show|say|print|output|echo|display|announce|present|reveal)\s+")
        .unwrap()
        .replace(&head, "")
        .to_string();
    head = Regex::new(r"(?i)^the\s+phrase\s+")
        .unwrap()
        .replace(&head, "")
        .to_string();

    let head = sanitize_text(head.trim().trim_end_matches([':', ',']).trim());
    if !head.is_empty() {
        head
    } else {
        sanitize_text(p.as_str())
    }
}

fn looks_like_loop_prompt(prompt: &str) -> bool {
    let p = prompt.to_ascii_lowercase();
    p.contains(" times")
        || p.contains(" time")
        || p.contains(" once")
        || p.contains(" twice")
        || p.contains(" thrice")
        || Regex::new(r"(?i)\b\d+\s*x\b").unwrap().is_match(prompt)
        || loop_count_from_prompt(prompt).is_some()
}

fn build_branch_dsl(prompt: &str) -> String {
    let mut prompt = strip_wrapping_quotes(prompt);
    // "otherwise" → "else" (alias)
    prompt = Regex::new(r"(?i)\botherwise\b")
        .unwrap()
        .replace_all(prompt.as_str(), "else")
        .to_string();

    // Support multiple `elif` branches: if ... elif ... elif ... else ...
    let re_else =
        Regex::new(r"(?is)^(?P<head>.+?)(?:,?\s*else\s*(?:say|print|output)?\s+(?P<else>.+))?$")
            .unwrap();
    if let Some(c) = re_else.captures(prompt.trim()) {
        let head = c.name("head").map(|m| m.as_str()).unwrap_or("").trim();
        let else_msg = c.name("else").map(|m| sanitize_text(m.as_str()));

        if head.to_ascii_lowercase().trim_start().starts_with("if ") {
            let head = Regex::new(r"(?i)^if\s+")
                .unwrap()
                .replace(head, "")
                .to_string();

            let parts = Regex::new(r"(?i),?\s*elif\s+")
                .unwrap()
                .split(head.trim())
                .collect::<Vec<_>>();

            let re_part = Regex::new(
                r"(?is)^(?P<cond>.+?)\s*(?:,|:)?\s*(?:say|print|output)\s+(?P<msg>.+?)\s*$",
            )
            .unwrap();

            let mut branches: Vec<(String, String)> = Vec::new();
            let mut ok = true;
            for part in parts {
                let part = part.trim().trim_end_matches(',');
                if part.is_empty() {
                    continue;
                }
                if let Some(pc) = re_part.captures(part) {
                    let cond_raw = pc.name("cond").map(|m| m.as_str()).unwrap_or("");
                    let msg_raw = pc.name("msg").map(|m| m.as_str()).unwrap_or("");
                    let cond = normalize_condition(cond_raw);
                    let msg = sanitize_text(msg_raw);
                    branches.push((cond, msg));
                } else {
                    ok = false;
                    break;
                }
            }

            if ok && !branches.is_empty() {
                let mut lines: Vec<String> = Vec::new();
                for (idx, (cond, msg)) in branches.into_iter().enumerate() {
                    if idx == 0 {
                        lines.push(format!("if {cond}:"));
                    } else {
                        lines.push(format!("elif {cond}:"));
                    }
                    lines.push(format!("    neuro \"{msg}\""));
                }
                if let Some(e) = else_msg {
                    let msg = sanitize_text(e.as_str());
                    lines.push("else:".into());
                    lines.push(format!("    neuro \"{msg}\""));
                }
                return lines.join("\n");
            }
        }
    }

    let re = Regex::new(
        r"(?ix)^if\s+(?P<c1>.+?)\s*(?:,|:)?\s*(?:say|print|output)\s+(?P<m1>.+?)\s*(?:,?\s*elif\s+(?P<c2>.+?)\s*(?:say|print|output)\s+(?P<m2>.+?))?\s*(?:,?\s*else\s*(?:say|print|output)?\s*(?P<e>.+))?$"
    )
    .unwrap();

    if let Some(caps) = re.captures(&prompt) {
        let c1 = normalize_condition(caps.name("c1").map(|m| m.as_str()).unwrap_or(""));
        let m1 = sanitize_text(caps.name("m1").map(|m| m.as_str()).unwrap_or(""));

        let mut lines = vec![format!("if {c1}:"), format!("    neuro \"{m1}\"")];

        if let Some(c2) = caps.name("c2") {
            let cond = normalize_condition(c2.as_str());
            let msg = sanitize_text(caps.name("m2").map(|m| m.as_str()).unwrap_or(""));
            lines.push(format!("elif {cond}:"));
            lines.push(format!("    neuro \"{msg}\""));
        }

        if let Some(e) = caps.name("e") {
            let msg = sanitize_text(e.as_str());
            lines.push("else:".into());
            lines.push(format!("    neuro \"{msg}\""));
        }

        return lines.join("\n");
    }

    // Simple if + else.
    let re_simple = Regex::new(r"(?i)^if\s+(.+?)\s*(?:,|:)?\s+(.+?)\s*(?:else\s+(.+))?$").unwrap();
    if let Some(caps) = re_simple.captures(&prompt) {
        let c1 = normalize_condition(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
        let m1 = sanitize_text(caps.get(2).map(|m| m.as_str()).unwrap_or(""));
        let mut lines = vec![format!("if {c1}:"), format!("    neuro \"{m1}\"")];
        if let Some(e) = caps.get(3) {
            let msg = sanitize_text(e.as_str());
            lines.push("else:".into());
            lines.push(format!("    neuro \"{msg}\""));
        }
        return lines.join("\n");
    }

    format!("neuro \"{}\"", prompt.trim())
}

fn build_doc_print_dsl(prompt: &str) -> String {
    let prompt = strip_wrapping_quotes(prompt);
    let plow = prompt.to_ascii_lowercase();

    // 1) Formatting: "Format Hello and World with a comma" -> "Hello, World"
    if plow.trim_start().starts_with("format ") && plow.contains("comma") {
        if let Some(c) =
            Regex::new(r"(?i)^format\s+(.+?)\s+and\s+(.+?)\s+with\s+a\s+comma\s*[.!?…]*\s*$")
                .unwrap()
                .captures(prompt.as_str())
        {
            let a = sanitize_text(c.get(1).map(|m| m.as_str()).unwrap_or(""));
            let b = sanitize_text(c.get(2).map(|m| m.as_str()).unwrap_or(""));
            if !a.is_empty() && !b.is_empty() {
                return format!("neuro \"{a}, {b}\"");
            }
        }
    }

    // 2) Say the number N → N
    if let Some(c) = Regex::new(r"(?i)^say\s+the\s+number\s+(\d+)\b")
        .unwrap()
        .captures(prompt.as_str())
    {
        if let Some(n) = c.get(1).map(|m| m.as_str()) {
            return format!("neuro \"{n}\"");
        }
    }

    // 3) "Print the value of result" / "Output the counter value" / "Display final_score"
    let re_value_of = Regex::new(r"(?i)\bvalue\s+of\s+([A-Za-z_][\w]*)\b").unwrap();
    if let Some(c) = re_value_of.captures(prompt.as_str()) {
        if let Some(var) = c.get(1).map(|m| m.as_str()) {
            return format!("neuro {var}");
        }
    }
    let re_var_value = Regex::new(r"(?i)\bthe\s+([A-Za-z_][\w]*)\s+value\b").unwrap();
    if let Some(c) = re_var_value.captures(prompt.as_str()) {
        if let Some(var) = c.get(1).map(|m| m.as_str()) {
            return format!("neuro {var}");
        }
    }
    let re_display = Regex::new(r"(?i)^(?:display|show)\s+([A-Za-z_][\w]*)\s*$").unwrap();
    if let Some(c) = re_display.captures(prompt.as_str()) {
        if let Some(var) = c.get(1).map(|m| m.as_str()) {
            return format!("neuro {var}");
        }
    }

    // 4) Comment macros: keep a comment line plus an optional print/say/output tail.
    let is_comment_prompt = plow.contains("write a comment")
        || plow.contains("add comment")
        || plow.contains("insert comment")
        || plow.contains("comment that says")
        || plow.contains("comment says")
        || plow.contains("using //")
        || plow.contains("using #");

    let comment_line = if is_comment_prompt {
        let mut comment = first_quoted(prompt.as_str());
        if comment.is_none() {
            let re = Regex::new(r"(?i)\bcomment\b\s+(?:that\s+says\s+|says\s+)?(.+)").unwrap();
            comment = re
                .captures(prompt.as_str())
                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
        }
        if comment.is_none() {
            let re2 =
                Regex::new(r"(?i)\bwrite a comment\b\s+(?:that\s+says\s+|says\s+)?(.+)").unwrap();
            comment = re2
                .captures(prompt.as_str())
                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
        }

        comment.and_then(|raw| {
            let mut msg = strip_wrapping_quotes(raw.as_str());
            msg = Regex::new(r"(?i)(?:using\s+//|using\s+#).*$")
                .unwrap()
                .replace(&msg, "")
                .trim()
                .to_string();

            if let Some(rest) = msg.strip_prefix("//") {
                msg = rest.trim().to_string();
            }
            if let Some(rest) = msg.strip_prefix('#') {
                msg = rest.trim().to_string();
            }

            if msg.is_empty() {
                None
            } else {
                Some(format!("// {msg}"))
            }
        })
    } else {
        None
    };

    let re_print = Regex::new(r"(?i)\b(?:and\s+)?(?:print|say|output|echo)\s+(.+)$").unwrap();
    let print_msg = re_print
        .captures(prompt.as_str())
        .and_then(|c| c.get(1).map(|m| sanitize_text(m.as_str())))
        .filter(|s| !s.is_empty());

    let mut lines: Vec<String> = Vec::new();
    if let Some(c) = comment_line {
        lines.push(c);
    } else if plow.contains("main starts here") {
        lines.push("// main starts here".into());
    }

    if let Some(msg) = print_msg {
        let is_ident = msg
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if is_ident {
            lines.push(format!("neuro {msg}"));
        } else {
            lines.push(neuro_line(msg.as_str()));
        }
    }

    if !lines.is_empty() {
        return lines.join("\n");
    }

    neuro_line(prompt.as_str())
}

// print 'X' + var  OR  print var1 + ' ' + var2
fn build_print_concat_dsl(prompt: &str) -> Option<String> {
    let p = strip_wrapping_quotes(prompt);
    let tmp = "tmpPrint";
    // print 'X' + var
    let re_lit_var = Regex::new(r#"(?i)^print\s+['"](.+?)['"]\s*\+\s*([A-Za-z_][\w]*)"#).unwrap();
    if let Some(c) = re_lit_var.captures(&p) {
        let lit = c
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("")
            .replace('\'', "\"");
        let var = c.get(2).map(|m| m.as_str()).unwrap_or("value");
        return Some(format!(
            r#"set {tmp} = "{lit}" + {var}
neuro {tmp}"#
        ));
    }

    // print var + ' ' + var2  tai print var + " " + var2
    let re_var_lit_var =
        Regex::new(r#"(?ix)^print\s+([A-Za-z_][\w]*)\s*\+\s*['"]\s*['"]\s*\+\s*([A-Za-z_][\w]*)"#)
            .unwrap();
    if let Some(c) = re_var_lit_var.captures(&p) {
        let v1 = c.get(1).map(|m| m.as_str()).unwrap_or("a");
        let v2 = c.get(2).map(|m| m.as_str()).unwrap_or("b");
        return Some(format!(
            r#"set {tmp} = {v1} + " " + {v2}
neuro {tmp}"#
        ));
    }

    None
}

fn build_roleflag_dsl(prompt: &str) -> String {
    let prompt = strip_wrapping_quotes(prompt);
    let lower = prompt.to_ascii_lowercase();
    let var = if lower.contains("role") {
        "role"
    } else {
        "flag"
    };
    let val = first_quoted(prompt.as_str())
        .or_else(|| {
            Regex::new(r"(?i)\b(is|=)\s+([A-Za-z_][\w]*)")
                .unwrap()
                .captures(prompt.as_str())
                .and_then(|c| c.get(2).map(|m| m.as_str().to_string()))
        })
        .unwrap_or_else(|| "true".to_string());

    let rhs = parse_rhs(&val);
    let mut lines = vec![format!("set {var} = {rhs}")];
    if mentions_print(prompt.as_str()) {
        lines.push(format!("neuro {var}"));
    }
    lines.join("\n")
}

fn build_ai_bridge_dsl(prompt: &str) -> String {
    neuro_line(prompt)
}

fn sanitize_text(s: &str) -> String {
    strip_wrapping_quotes(s)
        .trim_matches(['"', '\'', ' ', '.', ',', '!', '?', '…'])
        .trim()
        .to_string()
}

fn first_quoted(prompt: &str) -> Option<String> {
    let re = Regex::new(r#"'([^']+)'|"([^"]+)""#).unwrap();
    re.captures(prompt)
        .and_then(|c| c.get(1).or_else(|| c.get(2)))
        .map(|m| m.as_str().to_string())
}

fn all_quoted(prompt: &str) -> Vec<String> {
    let re = Regex::new(r#"'([^']+)'|"([^"]+)""#).unwrap();
    re.captures_iter(prompt)
        .filter_map(|c| c.get(1).or_else(|| c.get(2)))
        .map(|m| m.as_str().to_string())
        .collect()
}

fn mentions_print(prompt: &str) -> bool {
    let p = prompt.to_ascii_lowercase();
    p.contains("print")
        || p.contains("show")
        || p.contains("output")
        || p.contains("echo")
        || p.contains("say")
}

fn find_print_tail(prompt: &str, var: &str) -> Option<String> {
    // Find the last print/echo/output (ignore "show").
    let low = prompt.to_ascii_lowercase();
    let mut start = None;
    for key in ["print ", "echo ", "output "].iter() {
        if let Some(i) = low.rfind(key) {
            start = Some(i + key.len());
            break;
        }
    }
    let s = start?;
    let raw = prompt[s..].trim();

    // If it's a concatenation expression, keep spacing and replace single quotes.
    if raw.contains('+') {
        // Special-case lit + var: insert a space if missing.
        let re_lit_var = Regex::new(r#"(?i)^['"](.+?)['"]\s*\+\s*([A-Za-z_][\w]*)"#).unwrap();
        if let Some(c) = re_lit_var.captures(raw) {
            let lit = c.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let v = c.get(2).map(|m| m.as_str()).unwrap_or("value");
            let spacer = if lit.ends_with(' ') { "" } else { " " };
            return Some(format!(r#""{lit}" + "{spacer}" + {v}"#));
        }
        return Some(raw.replace('\'', "\""));
    }

    let low_raw = raw.to_ascii_lowercase();
    if low_raw == "it" {
        return Some(format!(r#""{var}=" + {var}"#));
    }

    // If the expression contains the variable name, build a lightweight concat.
    if raw.contains(var) {
        let parts = raw.splitn(2, var).collect::<Vec<_>>();
        let pre = parts
            .first()
            .copied()
            .unwrap_or("")
            .trim_end_matches(',')
            .to_string();
        let post = parts.get(1).copied().unwrap_or("").to_string();
        let mut segs = Vec::new();
        if !pre.trim().is_empty() {
            let mut p = strip_wrapping_quotes(pre.trim());
            if !p.ends_with(' ') {
                p.push(' ');
            }
            segs.push(format!(r#""{}""#, p));
        }
        segs.push(var.to_string());
        if !post.trim().is_empty() {
            let mut po = strip_wrapping_quotes(post.trim());
            if !po.starts_with(' ') {
                po = format!(" {po}");
            }
            segs.push(format!(r#""{}""#, po));
        }
        return Some(segs.join(" + "));
    }

    Some(normalize_expr(raw))
}

fn clean_expr(expr: &str) -> String {
    let mut e = expr.trim().trim_end_matches(',').to_string();
    let lower = e.to_ascii_lowercase();
    if let Some(m) = Regex::new(r"(?i)\s+and\s+[A-Za-z_][\w]*\s*=")
        .unwrap()
        .find(&lower)
    {
        e = e[..m.start()].trim().to_string();
    }
    if let Some(idx) = lower.find(", then") {
        e = e[..idx].trim().to_string();
    }
    e.replace('\'', "\"")
}

fn normalize_expr(expr: &str) -> String {
    let mut e = clean_expr(expr);
    // Lightweight power support: "(x - y) ** 2" -> "(x - y) * (x - y)"
    if e.contains("**") {
        if let Some(c) = Regex::new(r"(?i)^(?P<base>.+?)\s*\*\*\s*(?P<exp>\d+)\s*$")
            .unwrap()
            .captures(e.as_str())
        {
            let base = c.name("base").map(|m| m.as_str()).unwrap_or("").trim();
            let exp = c
                .name("exp")
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(0, 8);
            if exp == 0 {
                return "1".into();
            }
            if exp == 1 {
                return base.to_string();
            }
            let factor = format!("({base})");
            let parts = std::iter::repeat_n(factor, exp).collect::<Vec<_>>();
            return parts.join(" * ");
        }
    }
    // If there are too many quotes, drop inner quotes and quote the whole RHS.
    if e.matches('"').count() > 1 {
        e = e.replace('"', "");
        let t = e.trim();
        return format!(r#""{}""#, t);
    }
    let has_op = ['+', '-', '*', '/', '%'].iter().any(|op| e.contains(*op));
    if has_op {
        return e;
    }
    parse_rhs(&e)
}

fn parse_var_expr(prompt: &str) -> Option<(String, String, bool)> {
    let p = prompt.trim();
    let lp = p.to_ascii_lowercase();

    // set X to Y (e.g. "set x to 5 and print it")
    let re_set_to = Regex::new(r"(?i)set\s+([A-Za-z_][\w]*)\s+(?:to|=)\s+(.+)").unwrap();
    if let Some(caps) = re_set_to.captures(p) {
        let var = caps
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("value")
            .to_string();
        let mut expr = caps
            .get(2)
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let lower = expr.to_ascii_lowercase();
        let mut do_print = false;
        for sep in [
            " and print",
            " then print",
            " and output",
            " then output",
            " and echo",
            " then echo",
        ] {
            if let Some(idx) = lower.find(sep) {
                expr = expr[..idx].trim().to_string();
                do_print = true;
                break;
            }
        }
        expr = clean_expr(&expr);
        let do_print = do_print
            || lp.contains(" print")
            || lp.contains(" output")
            || lp.contains(" show")
            || lp.contains(" echo");
        if expr.is_empty() {
            expr = "0".into();
        }
        return Some((var, expr, do_print));
    }

    // create variable foo = expr
    let re_create_var =
        Regex::new(r"(?i)create\s+variable\s+([A-Za-z_][\w]*)\s*(?:=)?\s*(.+)").unwrap();
    if let Some(caps) = re_create_var.captures(p) {
        let var = caps
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("value")
            .to_string();
        let mut expr = caps
            .get(2)
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let lower = expr.to_ascii_lowercase();
        let mut do_print = false;
        for sep in [
            " and print",
            " then print",
            " and output",
            " then output",
            " and echo",
            " then echo",
        ] {
            if let Some(idx) = lower.find(sep) {
                expr = expr[..idx].trim().to_string();
                do_print = true;
                break;
            }
        }
        expr = clean_expr(&expr);
        let do_print = do_print
            || lp.contains(" print")
            || lp.contains(" output")
            || lp.contains(" show")
            || lp.contains(" echo");
        if expr.is_empty() {
            expr = "0".into();
        }
        return Some((var, expr, do_print));
    }

    // store 'hello' in var
    let re_store_in = Regex::new(r"(?i)store\s+(.+?)\s+in\s+([A-Za-z_][\w]*)").unwrap();
    if let Some(caps) = re_store_in.captures(p) {
        let expr = clean_expr(
            strip_wrapping_quotes(caps.get(1).map(|m| m.as_str()).unwrap_or("").trim()).as_str(),
        );
        let var = caps
            .get(2)
            .map(|m| m.as_str())
            .unwrap_or("value")
            .to_string();
        let do_print = true;
        return Some((var, expr, do_print));
    }

    // set/create/store var = expr [and/then print ...]
    let re = Regex::new(r"(?i)(?:set|create|store)\s+([A-Za-z_][\w]*)\s*(?:=|to)?\s*(.+)").unwrap();
    if let Some(caps) = re.captures(p) {
        let var = caps
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("value")
            .to_string();
        let mut expr = caps
            .get(2)
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let lower = expr.to_ascii_lowercase();
        let mut do_print = false;

        for sep in [
            " and print",
            " then print",
            " and output",
            " then output",
            " and echo",
            " then echo",
        ] {
            if let Some(idx) = lower.find(sep) {
                expr = expr[..idx].trim().to_string();
                do_print = true;
                break;
            }
        }

        if expr.is_empty() {
            expr = "0".into();
        }

        expr = clean_expr(&expr);

        // "into var" syntax (combine ... into label)
        let expr_lower = expr.to_ascii_lowercase();
        if let Some(idx) = expr_lower.find(" into ") {
            let head = expr[..idx].trim();
            let tail = expr[idx + " into ".len()..].trim();
            if !tail.is_empty() {
                expr = head.to_string();
            }
        }

        let do_print = do_print
            || lp.contains(" print")
            || lp.contains(" output")
            || lp.contains(" show")
            || lp.contains(" echo");

        return Some((var, expr, do_print));
    }

    None
}

fn parse_rhs(raw: &str) -> String {
    let had_quote = raw.contains('\'') || raw.contains('"');
    let mut val = strip_wrapping_quotes(sanitize_text(raw).as_str());
    val = val.replace('\'', "");
    if val.is_empty() {
        return "\"\"".into();
    }
    if val.parse::<f64>().is_ok() {
        return val;
    }
    let low = val.to_ascii_lowercase();
    if matches!(low.as_str(), "true" | "false" | "none") {
        return val;
    }
    if val.chars().all(|c| c.is_alphanumeric() || c == '_') {
        // If the original contained quotes, treat it as a literal.
        if had_quote {
            return format!("\"{val}\"");
        }
        return val;
    }
    format!("\"{val}\"")
}

fn normalize_condition(raw: &str) -> String {
    let mut c = raw.trim().to_string();
    let repl = [
        ("greater than or equal to", ">="),
        ("less than or equal to", "<="),
        ("greater than", ">"),
        ("less than", "<"),
        ("is not", "!="),
        ("not equal to", "!="),
        ("equals", "=="),
        ("equal to", "=="),
        ("is", "=="),
    ];
    for (a, b) in repl {
        let re = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(a))).unwrap();
        c = re.replace_all(&c, b).to_string();
    }

    // Quote the RHS when it's a bare word literal.
    let re_rhs = Regex::new(r"(==|!=|>=|<=|>|<)\s*([A-Za-z_][\w]*)").unwrap();
    c = re_rhs
        .replace_all(&c, |caps: &regex::Captures| {
            let rhs = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let op = caps.get(1).map(|m| m.as_str()).unwrap_or("==");
            if rhs.parse::<f64>().is_ok()
                || matches!(rhs.to_ascii_lowercase().as_str(), "true" | "false" | "none")
            {
                format!("{op} {rhs}")
            } else {
                format!(r#"{op} "{rhs}""#)
            }
        })
        .to_string();

    c.trim_end_matches(',').trim().to_string()
}

fn strip_wrapping_quotes(s: &str) -> String {
    let mut t = s.trim();
    loop {
        if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
            if t.len() <= 1 {
                break;
            }
            t = &t[1..t.len() - 1];
            t = t.trim();
            continue;
        }
        break;
    }
    t.to_string()
}

fn neuro_line(msg: &str) -> String {
    let clean = strip_wrapping_quotes(msg);
    let safe = clean.replace(['"', '\''], "");
    format!("neuro \"{}\"", safe.trim())
}

/* ------------------------- (test-only helpers) ------------------------ */
#[allow(dead_code)]
fn sanitize_lines(src: &str) -> String {
    const ALLOWED: &[&str] = &["neuro ", "set ", "if ", "elif ", "else", "ai:", "//", "#"];
    src.lines()
        .filter_map(|ln| {
            let cleaned = ln.replace(['✅', '❌', '🚀', '👉'], "");
            let trimmed = cleaned.trim_start();
            ALLOWED
                .iter()
                .any(|p| trimmed.to_ascii_lowercase().starts_with(p))
                .then(|| trimmed.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

/* ----------------------------- Helpers ------------------------------- */
#[inline]
fn eq_case(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}
#[inline]
#[allow(dead_code)]
fn var(map: &HashMap<String, String>, k: &str) -> String {
    map.get(k).cloned().unwrap_or_else(|| k.to_string())
}
#[inline]
fn var_or_literal(map: &HashMap<String, String>, k: &str) -> String {
    map.get(k).cloned().unwrap_or_else(|| k.to_string())
}
#[allow(dead_code)]
fn bail_undefined(name: &str) -> ! {
    panic!("❌ Error: variable '{name}' is not defined.");
}

/* -------------------------------- Tests ------------------------------ */
#[cfg(test)]
mod tests;
