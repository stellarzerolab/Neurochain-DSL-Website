//! Unit tests for the NeuroChain lexer (tokenizer).

use super::{tokenize, Token};

#[test]
fn tokenizes_macro_from_ai_single_line() {
    let src = r#"macro from AI: "Show Ping 2 times""#;
    let toks = tokenize(src).unwrap();
    assert_eq!(
        toks,
        vec![
            Token::Macro,
            Token::From,
            Token::AI,
            Token::Colon,
            Token::String("\"Show Ping 2 times\"".to_string()),
            Token::Newline,
        ]
    );
}

#[test]
fn strips_inline_comment_outside_quotes() {
    let src = r#"neuro "Hello" # comment"#;
    let toks = tokenize(src).unwrap();
    assert_eq!(
        toks,
        vec![
            Token::Neuro,
            Token::String("\"Hello\"".to_string()),
            Token::Newline,
        ]
    );
}

#[test]
fn keeps_comment_markers_inside_quotes() {
    let src = r#"neuro "Hello # not a comment""#;
    let toks = tokenize(src).unwrap();
    assert_eq!(
        toks,
        vec![
            Token::Neuro,
            Token::String("\"Hello # not a comment\"".to_string()),
            Token::Newline,
        ]
    );
}

#[test]
fn tokenizes_indent_and_dedent() {
    let src = r#"
if x == 1:
    neuro "OK"
neuro "Done"
"#;
    let toks = tokenize(src).unwrap();
    assert_eq!(
        toks.iter().filter(|t| matches!(t, Token::Indent)).count(),
        1
    );
    assert_eq!(
        toks.iter().filter(|t| matches!(t, Token::Dedent)).count(),
        1
    );
}

#[test]
fn tokenizes_parentheses() {
    let src = r#"set r = (a + b) * 2"#;
    let toks = tokenize(src).unwrap();
    assert!(toks.iter().any(|t| matches!(t, Token::LParen)));
    assert!(toks.iter().any(|t| matches!(t, Token::RParen)));
}
