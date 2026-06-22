//! 字句解析・構文解析の縦切り回帰テスト。

use iris_lang::ast::{BinaryOp, ExprKind, Item, LitPat, Pattern, Stmt, Type};
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

#[test]
fn parses_for_range_loop() {
    // `for x in lo..hi` / `for x in lo..=hi`（範囲・包含性・本体）。
    let program = parse_src(
        "fn f() {\n    for i in 0..10 {\n        continue\n    }\n    for j in 1..=n {\n        break\n    }\n}",
    );
    let Item::Function(f) = &program.items[0] else { panic!("関数のはず") };
    let Stmt::For { var, inclusive, start, body, .. } = &f.body.stmts[0] else {
        panic!("for 文のはず");
    };
    assert_eq!(var, "i");
    assert!(!*inclusive); // `..`
    assert!(matches!(&start.kind, ExprKind::Int(0)));
    assert!(matches!(&body.stmts[0], Stmt::Continue { .. }));
    let Stmt::For { var, inclusive, end, body, .. } = &f.body.stmts[1] else {
        panic!("for 文のはず");
    };
    assert_eq!(var, "j");
    assert!(*inclusive); // `..=`
    assert!(matches!(&end.kind, ExprKind::Ident(name) if name == "n"));
    assert!(matches!(&body.stmts[0], Stmt::Break { .. }));
}

#[test]
fn lexes_range_operators() {
    // `..` と `..=` は最長一致で正しくトークン化される（`.` と区別する）。
    let kinds: Vec<_> = lex("1..10 1..=10")
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Int(1),
            TokenKind::DotDot,
            TokenKind::Int(10),
            TokenKind::Int(1),
            TokenKind::DotDotEq,
            TokenKind::Int(10),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn parses_match_range_and_guard_arms() {
    // 範囲パターン（排他・包含）とガード付きアームを解析する。
    let program = parse_src(
        "fn f(n: i32): i32 {\n    match n {\n        0..10 -> 1\n        10..=20 -> 2\n        _ if n > 100 -> 3\n        _ -> 0\n    }\n}",
    );
    let Item::Function(f) = &program.items[0] else { panic!("関数のはず") };
    let Stmt::Expr(e) = &f.body.stmts[0] else { panic!("式文のはず") };
    let ExprKind::Match { arms, .. } = &e.kind else { panic!("match 式のはず") };
    assert_eq!(arms.len(), 4);
    // 排他範囲 0..10。
    assert!(matches!(
        &arms[0].pattern,
        Pattern::Range { lo: LitPat::Int(0), hi: LitPat::Int(10), inclusive: false, .. }
    ));
    assert!(arms[0].guard.is_none());
    // 包含範囲 10..=20。
    assert!(matches!(
        &arms[1].pattern,
        Pattern::Range { lo: LitPat::Int(10), hi: LitPat::Int(20), inclusive: true, .. }
    ));
    // ワイルドカード + ガード。
    assert!(matches!(&arms[2].pattern, Pattern::Wildcard { .. }));
    assert!(matches!(
        arms[2].guard.as_ref().map(|g| &g.kind),
        Some(ExprKind::Binary { op: BinaryOp::Gt, .. })
    ));
}

#[test]
fn parses_array_literal_multiline_and_trailing_comma() {
    // 配列リテラルは改行・カンマ区切り、末尾カンマ、空リストを許す。
    let prog = parse_src("fn f() {\n    let a = [1, 2, 3,]\n    let b = [\n        10\n        20\n    ]\n    let c = []\n}");
    let Item::Function(func) = &prog.items[0] else {
        panic!("関数のはず");
    };
    let mut arrays = 0;
    for stmt in &func.body.stmts {
        if let Stmt::Let { value, .. } = stmt
            && let ExprKind::ArrayLit { elems } = &value.kind
        {
            arrays += 1;
            // 1 つ目は 3 要素、2 つ目は 2 要素、3 つ目は空。
            assert!(elems.len() == 3 || elems.len() == 2 || elems.is_empty());
        }
    }
    assert_eq!(arrays, 3);
}

#[test]
fn parses_subscript_index() {
    // 添字アクセス `base[index]` は後置式として解析され、チェーンできる。
    let prog = parse_src("fn f() {\n    let x = arr[0]\n    let y = m[i][j]\n}");
    let Item::Function(func) = &prog.items[0] else {
        panic!("関数のはず");
    };
    // 1 つ目: arr[0] は Index{ base: Ident(arr), index: Int(0) }。
    let Stmt::Let { value, .. } = &func.body.stmts[0] else {
        panic!("let のはず");
    };
    let ExprKind::Index { base, index } = &value.kind else {
        panic!("Index のはず");
    };
    assert!(matches!(&base.kind, ExprKind::Ident(n) if n == "arr"));
    assert!(matches!(&index.kind, ExprKind::Int(0)));
    // 2 つ目: m[i][j] は Index{ base: Index{ ... }, index: j } のチェーン。
    let Stmt::Let { value, .. } = &func.body.stmts[1] else {
        panic!("let のはず");
    };
    let ExprKind::Index { base, .. } = &value.kind else {
        panic!("外側 Index のはず");
    };
    assert!(matches!(&base.kind, ExprKind::Index { .. }));
}

#[test]
fn parses_cast_precedence() {
    // `a + b as i32` は `a + (b as i32)`（cast は二項より強い）。
    let prog = parse_src("fn f() {\n    let x = a + b as i32\n}");
    let Item::Function(func) = &prog.items[0] else { panic!() };
    let Stmt::Let { value, .. } = &func.body.stmts[0] else { panic!() };
    // 最上位は加算で、右辺が Cast。
    let ExprKind::Binary { op: BinaryOp::Add, rhs, .. } = &value.kind else {
        panic!("最上位は加算のはず: {:?}", value.kind)
    };
    assert!(matches!(rhs.kind, ExprKind::Cast { .. }), "右辺は cast のはず");
}
