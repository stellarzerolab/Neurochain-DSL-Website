//! NeuroChain parser.
//!
//! Converts the lexer token stream into an AST.
//! Supports model selection (`AI: "path.onnx"`), variables (`set ...`), control-flow
//! (`if`/`elif`/`else` with indentation), and macro calls (`macro from AI: ...`).

use std::iter::{IntoIterator, Peekable};
use std::vec::IntoIter;

use crate::lexer::Token;

/* ------------------------------- AST ------------------------------- */
#[derive(Debug, PartialEq, Clone)]
pub enum Expr {
    StringLit(String), // "Positive"
    Value(String),     // Identifier or number (a, 42).
    BinaryOp(Box<Expr>, BinaryOperator, Box<Expr>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
}

#[derive(Debug, PartialEq)]
pub enum ASTNode {
    AIModel(String),
    Neuro(String), // Unified output command.
    SetVar(String, Expr),
    SetVarFromAI(String, String),
    MacroCall(String), // `macro from AI: ...`
    IfStatement {
        condition: BoolExpr,
        body: Vec<ASTNode>,
        elif_blocks: Vec<(BoolExpr, Vec<ASTNode>)>,
        else_body: Option<Vec<ASTNode>>,
    },
}

#[derive(Debug, PartialEq)]
pub enum BoolExpr {
    Equals(String, String),
    NotEquals(String, String),
    EqualsVar(String, String),
    NotEqualsVar(String, String),
    VarEqualsVar(String, String),
    VarNotEqualsVar(String, String),
    Greater(String, String),
    GreaterEqual(String, String),
    Less(String, String),
    LessEqual(String, String),
    And(Box<BoolExpr>, Box<BoolExpr>),
    Or(Box<BoolExpr>, Box<BoolExpr>),
}

/* ------------------------------ PARSER ------------------------------ */
pub fn parse(tokens: Vec<Token>) -> Vec<ASTNode> {
    let mut ast = Vec::new();
    let mut it = tokens.into_iter().peekable();

    while it.peek().is_some() {
        match parse_statement(&mut it) {
            Some(node) => ast.push(node),
            None => {
                it.next();
            } // Drop unknown token.
        }
    }
    ast
}

/* ---------- statement ---------- */
fn parse_statement(it: &mut Peekable<IntoIter<Token>>) -> Option<ASTNode> {
    match it.peek()? {
        /* Model selection: AI: "..." */
        Token::AI => {
            it.next();
            expect(Token::Colon, it)?;
            if let Some(Token::String(path)) = it.next() {
                return Some(ASTNode::AIModel(path));
            }
        }

        /* neuro "..." */
        Token::Neuro => {
            it.next();
            if let Some(Token::String(text)) = it.next() {
                return Some(ASTNode::Neuro(text));
            }
        }

        /* set ... */
        Token::Set => {
            it.next();
            if let Some(Token::String(var)) = it.next() {
                match it.peek() {
                    Some(Token::EqualsAssign) => {
                        it.next();
                        let expr = parse_expr(it)?;
                        return Some(ASTNode::SetVar(var, expr));
                    }
                    Some(Token::From) => {
                        it.next(); // from
                        expect(Token::AI, it)?;
                        expect(Token::Colon, it)?;
                        if let Some(Token::String(prompt)) = it.next() {
                            return Some(ASTNode::SetVarFromAI(var, prompt));
                        }
                    }
                    _ => {}
                }
            }
        }

        /* macro from AI: ... */
        Token::Macro => {
            it.next(); // macro
            expect(Token::From, it)?; // from
            expect(Token::AI, it)?; // AI
            expect(Token::Colon, it)?; // :

            // Collect tokens until newline/dedent (macro prompt is on the same line).
            let mut parts = Vec::new();
            loop {
                match it.peek() {
                    Some(Token::Newline) | Some(Token::Dedent) | None => break,
                    Some(tok) => {
                        // Preserve original token text (string or number).
                        let txt = match tok {
                            Token::String(s) => s.clone(),
                            Token::Number(n) => n.clone(),
                            _ => break, // Unexpected token type -> stop.
                        };
                        parts.push(txt);
                        it.next(); // Advance to the next token.
                    }
                }
            }
            if !parts.is_empty() {
                let instr = parts.join(" ");
                return Some(ASTNode::MacroCall(instr));
            }
        }

        /* if/elif/else */
        Token::If => {
            it.next();
            let cond = parse_bool_expr(it)?;
            expect(Token::Colon, it)?;
            skip_newlines(it);
            expect(Token::Indent, it)?;
            let body = parse_block(it);

            let mut elifs = Vec::new();
            while matches!(it.peek(), Some(Token::Elif)) {
                it.next();
                let c = parse_bool_expr(it)?;
                expect(Token::Colon, it)?;
                skip_newlines(it);
                expect(Token::Indent, it)?;
                let b = parse_block(it);
                elifs.push((c, b));
            }

            let else_body = if matches!(it.peek(), Some(Token::Else)) {
                it.next();
                expect(Token::Colon, it)?;
                skip_newlines(it);
                expect(Token::Indent, it)?;
                Some(parse_block(it))
            } else {
                None
            };

            return Some(ASTNode::IfStatement {
                condition: cond,
                body,
                elif_blocks: elifs,
                else_body,
            });
        }

        /* Comment-only line */
        Token::Comment => {
            it.next();
            return None;
        }

        _ => {}
    }
    None
}

/* ---------- block ---------- */
fn parse_block(it: &mut Peekable<IntoIter<Token>>) -> Vec<ASTNode> {
    let mut block = Vec::new();
    loop {
        match it.peek() {
            Some(Token::Dedent) => {
                it.next();
                break;
            }
            Some(Token::Newline) => {
                it.next();
            }
            Some(_) => {
                if let Some(stmt) = parse_statement(it) {
                    block.push(stmt);
                } else {
                    it.next();
                }
            }
            None => break,
        }
    }
    block
}

/* ---------- boolean expr ---------- */
fn parse_bool_expr(it: &mut Peekable<IntoIter<Token>>) -> Option<BoolExpr> {
    let mut expr = parse_bool_atom(it)?;

    while let Some(tok) = it.peek() {
        let and = matches!(tok, Token::And);
        if !and && !matches!(tok, Token::Or) {
            break;
        }
        it.next();
        let rhs = parse_bool_atom(it)?;
        expr = if and {
            BoolExpr::And(Box::new(expr), Box::new(rhs))
        } else {
            BoolExpr::Or(Box::new(expr), Box::new(rhs))
        };
    }
    Some(expr)
}

fn parse_bool_atom(it: &mut Peekable<IntoIter<Token>>) -> Option<BoolExpr> {
    let take_value = |it: &mut Peekable<IntoIter<Token>>| -> Option<String> {
        match it.next()? {
            Token::Minus => match it.next()? {
                Token::Number(n) => Some(format!("-{}", n)),
                _ => None,
            },
            Token::String(s) => Some(s),
            Token::Number(n) => Some(n),
            _ => None,
        }
    };

    let l = take_value(it)?;
    let op = it.next()?;
    let r = take_value(it)?;
    let is_lit = |s: &str| s.starts_with('"') && s.ends_with('"');
    let strip = |s: &str| s.trim_matches('"').to_string();

    let strip_if_lit = |s: String| if is_lit(&s) { strip(&s) } else { s };

    match op {
        Token::Equals => Some(if !is_lit(&l) && !is_lit(&r) {
            BoolExpr::VarEqualsVar(l, r)
        } else if !is_lit(&l) {
            BoolExpr::EqualsVar(l, strip(&r))
        } else {
            BoolExpr::Equals(strip(&l), strip(&r))
        }),
        Token::NotEquals => Some(if !is_lit(&l) && !is_lit(&r) {
            BoolExpr::VarNotEqualsVar(l, r)
        } else if !is_lit(&l) {
            BoolExpr::NotEqualsVar(l, strip(&r))
        } else {
            BoolExpr::NotEquals(strip(&l), strip(&r))
        }),
        Token::GreaterThan => Some(BoolExpr::Greater(strip_if_lit(l), strip_if_lit(r))),
        Token::GreaterEqual => Some(BoolExpr::GreaterEqual(strip_if_lit(l), strip_if_lit(r))),
        Token::LessThan => Some(BoolExpr::Less(strip_if_lit(l), strip_if_lit(r))),
        Token::LessEqual => Some(BoolExpr::LessEqual(strip_if_lit(l), strip_if_lit(r))),
        _ => None,
    }
}

/* ---------- arithmetic expr ---------- */
/*  EBNF
    Expr   = Term   { ("+"|"-"|"=="|"!="|">"|"<"|">="|"<=") Term } ;
    Term   = Factor { ("*"|"/"|"%") Factor } ;
    Factor = Number
           | Ident
           | StringLit
           | "(" Expr ")" ;
*/
fn parse_expr(it: &mut Peekable<IntoIter<Token>>) -> Option<Expr> {
    let mut lhs = parse_term(it)?;

    loop {
        let op = match it.peek()? {
            Token::Plus => BinaryOperator::Add,
            Token::Minus => BinaryOperator::Sub,
            Token::GreaterThan => BinaryOperator::Gt,
            Token::LessThan => BinaryOperator::Lt,
            Token::GreaterEqual => BinaryOperator::Ge,
            Token::LessEqual => BinaryOperator::Le,
            Token::Equals => BinaryOperator::Eq,
            Token::NotEquals => BinaryOperator::Ne,
            _ => break,
        };
        it.next(); // Consume operator.
        let rhs = parse_term(it)?;
        lhs = Expr::BinaryOp(Box::new(lhs), op, Box::new(rhs));
    }
    Some(lhs)
}

fn parse_term(it: &mut Peekable<IntoIter<Token>>) -> Option<Expr> {
    let mut lhs = parse_factor(it)?;

    while let Some(op) = match it.peek()? {
        Token::Star => Some(BinaryOperator::Mul),
        Token::Slash => Some(BinaryOperator::Div),
        Token::Percent => Some(BinaryOperator::Mod),
        _ => None,
    } {
        it.next(); // Consume operator.
        let rhs = parse_factor(it)?;
        lhs = Expr::BinaryOp(Box::new(lhs), op, Box::new(rhs));
    }
    Some(lhs)
}

fn parse_factor(it: &mut Peekable<IntoIter<Token>>) -> Option<Expr> {
    match it.next()? {
        Token::Minus => {
            let inner = parse_factor(it)?;
            Some(Expr::BinaryOp(
                Box::new(Expr::Value("0".into())),
                BinaryOperator::Sub,
                Box::new(inner),
            ))
        }
        Token::Number(n) => Some(Expr::Value(n)),
        Token::String(s) if s.starts_with('"') && s.ends_with('"') => {
            Some(Expr::StringLit(s.trim_matches('"').to_string()))
        }
        Token::String(s) => Some(Expr::Value(s)),

        // Parentheses.
        Token::LParen => {
            let inner = parse_expr(it)?; // Recursive.
            expect(Token::RParen, it)?; // Require ')'.
            Some(inner)
        }
        _ => None,
    }
}

/* ---------- util ---------- */
fn skip_newlines(it: &mut Peekable<IntoIter<Token>>) {
    while matches!(it.peek(), Some(Token::Newline)) {
        it.next();
    }
}
fn expect(tok: Token, it: &mut Peekable<IntoIter<Token>>) -> Option<()> {
    matches!(it.next(), Some(t) if t == tok).then(|| ())
}

#[cfg(test)]
mod tests;
