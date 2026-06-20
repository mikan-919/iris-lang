//! 字句解析・構文解析の縦切り回帰テスト。

use iris_lang::ast::{BinaryOp, ExprKind, Item, Stmt, Type};
use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::token::TokenKind;

fn parse_src(src: &str) -> iris_lang::ast::Program {
    let tokens = lex(src).expect("字句解析に成功するはず");
    parse(&tokens).expect("構文解析に成功するはず")
}

#[test]
fn lexes_keywords_and_literals() {
    let tokens = lex("fn x = 42 1.5 \"hi\" true").unwrap();
    let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Fn,
            TokenKind::Ident("x".into()),
            TokenKind::Assign,
            TokenKind::Int(42),
            TokenKind::Float(1.5),
            TokenKind::Str("hi".into()),
            TokenKind::Bool(true),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexes_newlines_but_skips_comments() {
    let tokens = lex("a // コメント\nb").unwrap();
    let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Newline,
            TokenKind::Ident("b".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn parses_function_with_params_and_return_type() {
    let program = parse_src("fn add(a: i32, b: i32): i32 {\n    return a + b\n}");
    assert_eq!(program.items.len(), 1);
    let Item::Function(f) = &program.items[0] else { panic!("関数のはず") };
    assert_eq!(f.name, "add");
    assert_eq!(f.params.len(), 2);
    assert!(matches!(&f.ret, Some(Type::Named { name, .. }) if name == "i32"));
}

#[test]
fn respects_operator_precedence() {
    // 1 + 2 * 3 は 1 + (2 * 3) になる。
    let program = parse_src("fn f() {\n    let x = 1 + 2 * 3\n}");
    let Item::Function(f) = &program.items[0] else { panic!("関数のはず") };
    let Stmt::Let { value, .. } = &f.body.stmts[0] else {
        panic!("let 文のはず");
    };
    let ExprKind::Binary { op, rhs, .. } = &value.kind else {
        panic!("二項演算のはず");
    };
    assert_eq!(*op, BinaryOp::Add);
    assert!(matches!(
        &rhs.kind,
        ExprKind::Binary {
            op: BinaryOp::Mul,
            ..
        }
    ));
}

#[test]
fn parses_try_ternary_and_generics() {
    let program = parse_src(
        "fn f() {\n    let v: Vec<i32> = load()!\n    let y = cond ? 1 : 2\n}",
    );
    let Item::Function(f) = &program.items[0] else { panic!("関数のはず") };
    // let v: Vec<i32> = load()!
    let Stmt::Let { ty, value, .. } = &f.body.stmts[0] else {
        panic!("let 文のはず");
    };
    assert!(matches!(ty, Some(Type::Named { name, args, .. }) if name == "Vec" && args.len() == 1));
    assert!(matches!(&value.kind, ExprKind::Try(_)));
    // let y = cond ? 1 : 2
    let Stmt::Let { value, .. } = &f.body.stmts[1] else {
        panic!("let 文のはず");
    };
    assert!(matches!(&value.kind, ExprKind::Ternary { .. }));
}

#[test]
fn reports_error_with_span() {
    let tokens = lex("fn f() {\n    let x = \n}").unwrap();
    let err = parse(&tokens).expect_err("構文エラーになるはず");
    // `let x = ` の直後（改行）を指す。
    assert!(err.message.contains("式"));
}

#[test]
fn parses_while_loop_break_continue() {
    let program = parse_src(
        "fn f() {\n    while i < 10 {\n        continue\n    }\n    loop {\n        break\n    }\n}",
    );
    let Item::Function(f) = &program.items[0] else { panic!("関数のはず") };
    let Stmt::While { cond, body, .. } = &f.body.stmts[0] else {
        panic!("while 文のはず");
    };
    assert!(matches!(&cond.kind, ExprKind::Binary { op: BinaryOp::Lt, .. }));
    assert!(matches!(&body.stmts[0], Stmt::Continue { .. }));
    let Stmt::Loop { body, .. } = &f.body.stmts[1] else {
        panic!("loop 文のはず");
    };
    assert!(matches!(&body.stmts[0], Stmt::Break { .. }));
}
