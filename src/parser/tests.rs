//! Unit tests for the NeuroChain parser.

use super::*;
use crate::lexer::tokenize;

#[test]
fn parses_macro_call() {
    let src = r#"macro from AI: "Echo hello 2 times""#;
    let toks = tokenize(src).unwrap();
    let ast = parse(toks);
    assert!(matches!(ast[0], ASTNode::MacroCall(_)));
}

#[test]
fn parses_parenthesized_expr() {
    let src = r#"set r = (a + b) * 2"#;
    let toks = tokenize(src).unwrap();
    let ast = parse(toks);
    assert_eq!(ast.len(), 1);
}

#[test]
fn parses_if_else_block() {
    let src = r#"
set x = 1
if x == 1:
    neuro "OK"
else:
    neuro "NO"
"#;
    let toks = tokenize(src).unwrap();
    let ast = parse(toks);
    assert!(
        ast.iter().any(|n| matches!(
            n,
            ASTNode::IfStatement {
                else_body: Some(_),
                ..
            }
        )),
        "expected an if/else statement"
    );
}
