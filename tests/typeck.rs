//! 型検査パスの回帰テスト。

use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::sema::{check, resolve};

/// 型検査まで通す。エラーがあればメッセージ一覧を返す。
fn typecheck(src: &str) -> Result<(), Vec<String>> {
    let tokens = lex(src).expect("字句解析");
    let program = parse(&tokens).expect("構文解析");
    let resolution = resolve(&program).expect("名前解決");
    check(&program, &resolution)
        .map(|_| ())
        .map_err(|errs| errs.into_iter().map(|e| e.message).collect())
}

#[test]
fn accepts_well_typed_program() {
    let src = "fn add(a: i32, b: i32): i32 {\n    let s = a + b\n    return s\n}";
    typecheck(src).expect("型が整合しているはず");
}

#[test]
fn rejects_let_annotation_mismatch() {
    let errs = typecheck("fn f() {\n    let x: i32 = true\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("型が一致しません")));
}

#[test]
fn allows_int_literal_into_any_integer() {
    typecheck("fn f() {\n    let x: i64 = 1\n    let y: u8 = 2\n}").expect("リテラルは整数型へ適合");
}

#[test]
fn rejects_concrete_int_width_mismatch() {
    // i32 の変数を i64 へ代入はできない（リテラルではない具体型同士）。
    let errs = typecheck("fn f() {\n    let a: i32 = 1\n    let b: i64 = a\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("型が一致しません")));
}

#[test]
fn rejects_arithmetic_on_bool() {
    let errs = typecheck("fn f() {\n    let x = true + 1\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("算術演算")));
}

#[test]
fn comparison_yields_bool() {
    typecheck("fn f() {\n    let b: bool = 1 < 2\n}").expect("比較は bool");
}

#[test]
fn rejects_return_type_mismatch() {
    let errs = typecheck("fn f(): i32 {\n    return true\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("戻り値の型")));
}

#[test]
fn checks_call_arity_and_args() {
    let arity = typecheck("fn g(a: i32) {\n}\nfn f() {\n    g(1, 2)\n}").unwrap_err();
    assert!(arity.iter().any(|m| m.contains("引数は")));

    let argty = typecheck("fn g(a: i32) {\n}\nfn f() {\n    g(true)\n}").unwrap_err();
    assert!(argty.iter().any(|m| m.contains("引数の型")));
}

#[test]
fn try_requires_result_returning_fn() {
    // void を返す関数で `!` は使えない。
    let errs =
        typecheck("fn p(): Result<i32, string> {\n    return Ok(1)\n}\nfn f() {\n    let x = p()!\n}")
            .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("Result または Option を返す")));
}

#[test]
fn try_unwraps_result() {
    let src = "fn p(): Result<i32, string> {\n    return Ok(1)\n}\nfn f(): Result<i32, string> {\n    let x: i32 = p()!\n    return Ok(x)\n}";
    typecheck(src).expect("`!` は Result の中身 i32 を取り出す");
}

#[test]
fn rejects_try_on_non_result() {
    let errs = typecheck("fn f(): Result<i32, string> {\n    let x = 1!\n    return Ok(0)\n}")
        .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("Result または Option にのみ")));
}

#[test]
fn rejects_reassign_immutable() {
    let errs = typecheck("fn f() {\n    let x = 1\n    x = 2\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("再代入できません")));
}

#[test]
fn allows_reassign_mutable() {
    typecheck("fn f() {\n    let mut x = 1\n    x = 2\n}").expect("let mut は再代入可");
}

#[test]
fn rejects_ternary_branch_mismatch() {
    let errs = typecheck("fn f(c: bool) {\n    let x = c ? 1 : true\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("三項演算子の分岐")));
}

#[test]
fn accepts_while_with_bool_condition() {
    typecheck("fn f() {\n    let mut i = 0\n    while i < 3 {\n        i = i + 1\n    }\n}")
        .expect("bool 条件の while は通る");
}

#[test]
fn rejects_non_bool_while_condition() {
    let errs = typecheck("fn f() {\n    while 1 + 1 {\n    }\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("while の条件")));
}

#[test]
fn rejects_break_outside_loop() {
    let errs = typecheck("fn f() {\n    break\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("`break`")));
}

#[test]
fn rejects_continue_outside_loop() {
    let errs = typecheck("fn f() {\n    continue\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("`continue`")));
}

#[test]
fn accepts_break_continue_inside_loop() {
    typecheck("fn f() {\n    loop {\n        if true {\n            break\n        }\n        continue\n    }\n}")
        .expect("ループ内の break/continue は通る");
}

#[test]
fn example_hello_typechecks() {
    let src = include_str!("../examples/hello.iris");
    typecheck(src).expect("サンプルは型検査を通るはず");
}
