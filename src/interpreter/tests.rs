//! Unit tests for the NeuroChain interpreter.

use super::{extract_dsl, sanitize_lines, Interpreter};
use crate::parser::{ASTNode, BinaryOperator, Expr};

#[test]
fn test_interpreter_set_and_add() {
    let mut interp = Interpreter::new();

    let ast = vec![ASTNode::SetVar(
        "result".into(),
        Expr::BinaryOp(
            Box::new(Expr::Value("2".into())),
            BinaryOperator::Add,
            Box::new(Expr::Value("3".into())),
        ),
    )];

    interp.run(ast);
    assert_eq!(interp.variables.get("result"), Some(&"5".to_string()));
}

#[test]
fn test_interpreter_variable_use_in_expr() {
    let mut interp = Interpreter::new();
    interp.run(vec![ASTNode::SetVar("a".into(), Expr::Value("10".into()))]);

    let ast = vec![ASTNode::SetVar(
        "sum".into(),
        Expr::BinaryOp(
            Box::new(Expr::Value("a".into())),
            BinaryOperator::Add,
            Box::new(Expr::Value("5".into())),
        ),
    )];

    interp.run(ast);
    assert_eq!(interp.variables.get("sum"), Some(&"15".to_string()));
}

#[test]
fn test_interpreter_comparison_expr() {
    let mut interp = Interpreter::new();
    let ast = vec![ASTNode::SetVar(
        "cmp".into(),
        Expr::BinaryOp(
            Box::new(Expr::Value("7".into())),
            BinaryOperator::Gt,
            Box::new(Expr::Value("4".into())),
        ),
    )];

    interp.run(ast);
    assert_eq!(interp.variables.get("cmp"), Some(&"true".to_string()));
}

#[test]
fn test_interpreter_string_concat() {
    let mut interp = Interpreter::new();
    let ast = vec![ASTNode::SetVar(
        "combined".into(),
        Expr::BinaryOp(
            Box::new(Expr::StringLit("Hello".into())),
            BinaryOperator::Add,
            Box::new(Expr::StringLit("World".into())),
        ),
    )];

    interp.run(ast);
    assert_eq!(
        interp.variables.get("combined"),
        Some(&"HelloWorld".to_string())
    );
}

#[test]
fn test_interpreter_string_concat_with_variable() {
    let mut interp = Interpreter::new();
    interp.run(vec![ASTNode::SetVar(
        "name".into(),
        Expr::StringLit("Joe".into()),
    )]);

    let ast = vec![ASTNode::SetVar(
        "greeting".into(),
        Expr::BinaryOp(
            Box::new(Expr::StringLit("Hello,".into())),
            BinaryOperator::Add,
            Box::new(Expr::Value("name".into())),
        ),
    )];

    interp.run(ast);
    assert_eq!(
        interp.variables.get("greeting"),
        Some(&"Hello,Joe".to_string())
    );
}

#[test]
fn test_interpreter_divide_by_zero() {
    let mut interp = Interpreter::new();
    let ast = vec![ASTNode::SetVar(
        "error".into(),
        Expr::BinaryOp(
            Box::new(Expr::Value("10".into())),
            BinaryOperator::Div,
            Box::new(Expr::Value("0".into())),
        ),
    )];

    interp.run(ast);
    assert_eq!(interp.variables.get("error"), Some(&"NaN".to_string()));
}

#[test]
fn test_interpreter_hello_universe_slogan() {
    let mut interp = Interpreter::new();
    let ast = vec![ASTNode::SetVar(
        "slogan".into(),
        Expr::BinaryOp(
            Box::new(Expr::StringLit("Hello".into())),
            BinaryOperator::Add,
            Box::new(Expr::StringLit("Universe".into())),
        ),
    )];

    interp.run(ast);
    assert_eq!(
        interp.variables.get("slogan"),
        Some(&"HelloUniverse".to_string())
    );
}

#[test]
fn strip_and_sanitize() {
    let txt = "### Instruction:\nX\n### Response:\nmacro from AI: junk\n✅ neuro \"hi\"\nfoo";
    assert_eq!(sanitize_lines(&extract_dsl(txt)), "neuro \"hi\"");
}
