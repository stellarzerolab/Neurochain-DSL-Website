//! Lexer.
//!
//! Tokenizes NeuroChain source code:
//! - Strips inline comments (`#` and `//`) outside quotes
//! - Tracks indentation (`Indent`/`Dedent`)
//! - Produces the full token stream, including `macro from AI:`

/// Debug mode: enabled only in non-release builds (`cargo run` / `cargo test` without `--release`).
pub const DEBUG_MODE: bool = cfg!(debug_assertions);

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    AI,
    Neuro, // Unified output command (replaces Say/Print).
    Set,
    From,
    Macro, // `macro from AI: ...`
    If,
    Elif,
    Else,
    Colon,
    Equals,
    NotEquals,
    EqualsAssign,
    String(String),
    Number(String),
    Newline,
    Indent,
    Dedent,
    And,
    Or,
    Comment,

    // Arithmetic and comparison operators.
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    GreaterThan,
    LessThan,
    GreaterEqual,
    LessEqual,

    LParen,
    RParen,
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut indent_stack = vec![0];

    for (line_idx, raw_line) in input.lines().enumerate() {
        // Strip inline comments outside quotes.
        let mut in_quote = false;
        let mut cut_pos = raw_line.len();

        for (i, ch) in raw_line.char_indices() {
            match ch {
                '"' => in_quote = !in_quote,
                '#' if !in_quote => {
                    cut_pos = i;
                    break;
                }
                '/' if !in_quote && raw_line[i..].starts_with("//") => {
                    cut_pos = i;
                    break;
                }
                _ => (),
            }
        }

        let line = &raw_line[..cut_pos];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            tokens.push(Token::Comment);
            tokens.push(Token::Newline);
            continue;
        }

        // Indentation handling.
        let indent = raw_line.chars().take_while(|c| *c == ' ').count();
        match indent.cmp(indent_stack.last().unwrap()) {
            std::cmp::Ordering::Greater => {
                indent_stack.push(indent);
                tokens.push(Token::Indent);
            }
            std::cmp::Ordering::Less => {
                while indent < *indent_stack.last().unwrap() {
                    indent_stack.pop();
                    tokens.push(Token::Dedent);
                }
            }
            _ => {}
        }

        let chars: Vec<char> = trimmed.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            match chars[i] {
                ':' => {
                    tokens.push(Token::Colon);
                    i += 1;
                }
                '=' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                    tokens.push(Token::Equals);
                    i += 2;
                }
                '=' => {
                    tokens.push(Token::EqualsAssign);
                    i += 1;
                }
                '!' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                    tokens.push(Token::NotEquals);
                    i += 2;
                }

                '>' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                    tokens.push(Token::GreaterEqual);
                    i += 2;
                }
                '>' => {
                    tokens.push(Token::GreaterThan);
                    i += 1;
                }
                '<' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                    tokens.push(Token::LessEqual);
                    i += 2;
                }
                '<' => {
                    tokens.push(Token::LessThan);
                    i += 1;
                }

                '+' => {
                    tokens.push(Token::Plus);
                    i += 1;
                }
                '-' => {
                    tokens.push(Token::Minus);
                    i += 1;
                }
                '*' => {
                    tokens.push(Token::Star);
                    i += 1;
                }
                '/' => {
                    tokens.push(Token::Slash);
                    i += 1;
                }
                '%' => {
                    tokens.push(Token::Percent);
                    i += 1;
                }
                '(' => {
                    tokens.push(Token::LParen);
                    i += 1;
                }
                ')' => {
                    tokens.push(Token::RParen);
                    i += 1;
                }

                '"' => {
                    let start = i + 1;
                    if let Some(end) = chars[start..].iter().position(|&c| c == '"') {
                        let content: String = chars[start..start + end].iter().collect();

                        // Don't wrap model paths in quotes.
                        if content.ends_with(".onnx") {
                            tokens.push(Token::String(content));
                        } else {
                            tokens.push(Token::String(format!("\"{content}\"")));
                        }

                        i = start + end + 1;
                    } else {
                        return Err(format!(
                            "❌ Missing quote on line {}: {}",
                            line_idx + 1,
                            raw_line
                        ));
                    }
                }

                c if c.is_ascii_digit() => {
                    let start = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                        i += 1; // consume '.'
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    tokens.push(Token::Number(chars[start..i].iter().collect()));
                }

                c if c.is_alphabetic() => {
                    let start = i;
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    match word.to_lowercase().as_str() {
                        "if" => tokens.push(Token::If),
                        "elif" => tokens.push(Token::Elif),
                        "else" => tokens.push(Token::Else),
                        "neuro" => tokens.push(Token::Neuro),
                        "set" => tokens.push(Token::Set),
                        "from" => tokens.push(Token::From),
                        "macro" => tokens.push(Token::Macro),
                        "ai" => tokens.push(Token::AI),
                        "and" => tokens.push(Token::And),
                        "or" => tokens.push(Token::Or),
                        _ => tokens.push(Token::String(word)),
                    }
                }

                c if c.is_whitespace() => {
                    i += 1;
                }

                _ => {
                    return Err(format!(
                        "❌ Unexpected character '{}' on line {}: {}",
                        chars[i],
                        line_idx + 1,
                        raw_line
                    ));
                }
            }
        }

        tokens.push(Token::Newline);
    }

    // Close any remaining indentation levels.
    while indent_stack.len() > 1 {
        indent_stack.pop();
        tokens.push(Token::Dedent);
    }

    if DEBUG_MODE {
        println!("DEBUG TOKENS: {:?}", tokens);
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests;
