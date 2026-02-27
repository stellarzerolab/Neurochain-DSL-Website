use crate::interpreter::Interpreter;
use crate::lexer::tokenize;
use crate::parser::parse;

use anyhow::Result as AnyResult;
use std::panic::{catch_unwind, AssertUnwindSafe};

type StdResult<T, E> = std::result::Result<T, E>;

/* ───────────────────── Preprocessing ───────────────────── */

fn preprocess(input: &str) -> String {
    // 1) Remove BOM if present
    let s = input.strip_prefix('\u{feff}').unwrap_or(input);
    // 2) Normalize line endings CRLF/CR -> LF
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    // 3) Tabs -> 4 spaces
    s.replace('\t', "    ")
}

/* ───────────────────── Legacy normalization ───────────────────── */

/// Normalize legacy syntax before tokenization.
/// - Line-start `say`/`print` -> `neuro`
/// - After a colon `: say`/`: print` -> `: neuro`
/// - Inline control structures `if/elif/else: <command>` are expanded into a block
pub(crate) fn normalize_legacy(input: &str) -> String {
    let mut out = String::with_capacity(input.len());

    for line in input.lines() {
        // Compute leading indentation
        let trimmed = line.trim_start();
        let indent_len = line.len().saturating_sub(trimmed.len());
        let indent = &line[..indent_len];

        // 1) Line-start say/print -> neuro
        let mut converted = if trimmed.to_ascii_lowercase().starts_with("say")
            && trimmed
                .get(3..)
                .is_none_or(|rest| rest.is_empty() || rest.starts_with([' ', '"', '\'', '(']))
        {
            let rest = trimmed.get(3..).unwrap_or("");
            format!("{}neuro{}", indent, rest)
        } else if trimmed.to_ascii_lowercase().starts_with("print")
            && trimmed
                .get(5..)
                .is_none_or(|rest| rest.is_empty() || rest.starts_with([' ', '"', '\'', '(']))
        {
            let rest = trimmed.get(5..).unwrap_or("");
            format!("{}neuro{}", indent, rest)
        } else {
            line.to_string()
        };

        // 2) Inline if/elif/else: "if ...: <command>" -> two-line block
        if let Some(colon_idx) = converted.find(':') {
            let (head, tail_with_colon) = converted.split_at(colon_idx + 1); // includes ':'

            // Safe slice: if there is nothing after the colon, tail_trim = ""
            let tail_trim = if tail_with_colon.len() > 1 {
                tail_with_colon[1..].trim_start()
            } else {
                ""
            };

            let head_trim = head.trim_start();
            let is_ctrl = head_trim.to_ascii_lowercase().starts_with("if ")
                || head_trim.to_ascii_lowercase().starts_with("elif ")
                || head_trim.to_ascii_lowercase().starts_with("else");

            if is_ctrl && !tail_trim.is_empty() {
                // Normalize tail command: say/print -> neuro
                let norm_tail = if tail_trim.to_ascii_lowercase().starts_with("say")
                    && tail_trim.get(3..).is_none_or(|rest| {
                        rest.is_empty() || rest.starts_with([' ', '"', '\'', '('])
                    }) {
                    let rest = tail_trim.get(3..).unwrap_or("");
                    format!("neuro{}", rest)
                } else if tail_trim.to_ascii_lowercase().starts_with("print")
                    && tail_trim.get(5..).is_none_or(|rest| {
                        rest.is_empty() || rest.starts_with([' ', '"', '\'', '('])
                    })
                {
                    let rest = tail_trim.get(5..).unwrap_or("");
                    format!("neuro{}", rest)
                } else {
                    tail_trim.to_string()
                };

                // Write as two lines, add 4 spaces to the block
                converted = format!("{head}\n{indent}    {norm_tail}");
            }
        }

        // 3) Other ": say/print" occurrences - use safe replacement
        converted = replace_case_insensitive_safe(&converted, ": say", ": neuro");
        converted = replace_case_insensitive_safe(&converted, ": print", ": neuro");

        out.push_str(&converted);
        out.push('\n');
    }

    out
}

/// Safe ASCII case-insensitive replacement without out-of-bounds risk.
fn replace_case_insensitive_safe(haystack: &str, needle: &str, replacement: &str) -> String {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    let hl = h.len();
    let nl = n.len();

    if nl == 0 || nl > hl {
        return haystack.to_string();
    }

    let mut out = Vec::<u8>::with_capacity(hl);
    let mut i = 0usize;
    while i + nl <= hl {
        // Compare case-insensitively (ASCII)
        let mut eq = true;
        for k in 0..nl {
            let la = h[i + k].to_ascii_lowercase();
            let lb = n[k].to_ascii_lowercase();
            if la != lb {
                eq = false;
                break;
            }
        }
        if eq {
            out.extend_from_slice(replacement.as_bytes());
            i += nl;
        } else {
            out.push(h[i]);
            i += 1;
        }
    }
    out.extend_from_slice(&h[i..]);
    String::from_utf8(out).unwrap_or_else(|_| haystack.to_string())
}

/* ───────────────────────── Execution logic ───────────────────────── */

fn run_single_block(block: &str, interpreter: &mut Interpreter) -> StdResult<(), String> {
    let pre = preprocess(block);
    let norm = normalize_legacy(&pre);
    let tokens = tokenize(&norm)?;
    let ast = parse(tokens);
    match catch_unwind(AssertUnwindSafe(|| interpreter.run(ast))) {
        Ok(_) => Ok(()),
        Err(_) => Err("Runtime error (e.g. undefined variable).".to_string()),
    }
}

/// Runs a script by blocks (split on empty lines) for CLI file mode.
pub fn analyze_blocks(input: &str, interpreter: &mut Interpreter) -> StdResult<(), String> {
    let mut current_block = String::new();

    for line in input.lines() {
        if line.trim().is_empty() {
            if !current_block.trim().is_empty() {
                run_single_block(&current_block, interpreter)?;
                current_block.clear();
            }
            continue;
        }
        current_block.push_str(line);
        current_block.push('\n');
    }

    if !current_block.trim().is_empty() {
        run_single_block(&current_block, interpreter)?;
    }

    Ok(())
}

/// Run the entire input as one unit (primary mode for API and REPL).
pub fn analyze(input: &str, interpreter: &mut Interpreter) -> StdResult<String, String> {
    interpreter.clear_output();
    run_single_block(input, interpreter)?;
    let out = interpreter.take_output();
    if out.trim().is_empty() {
        Ok("Execution succeeded.".into())
    } else {
        Ok(out)
    }
}

/* ───────────────────────── Macro stub ───────────────────────── */

#[allow(dead_code)]
pub fn generate(prompt: &str) -> AnyResult<String> {
    Ok(format!(
        "# Generated DSL demo\nneuro \"Hello from NeuroChain\"\n# Prompt: {prompt}"
    ))
}
