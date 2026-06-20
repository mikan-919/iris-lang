//! 名前解決パスの回帰テスト。

use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::sema::{DefKind, resolve};

fn parse_src(src: &str) -> iris_lang::ast::Program {
    let tokens = lex(src).expect("字句解析に成功するはず");
    parse(&tokens).expect("構文解析に成功するはず")
}

#[test]
fn resolves_params_and_locals() {
    let program = parse_src("fn add(a: i32, b: i32): i32 {\n    let s = a + b\n    return s\n}");
    let res = resolve(&program).expect("名前解決に成功するはず");
    // a, b, s の 3 つの使用がすべて解決されている（a+b で a,b、return s で s）。
    assert_eq!(res.uses.len(), 3);
}

#[test]
fn allows_mutual_recursion() {
    // 後方で定義される関数を前方から呼べる（全関数名を先に登録するため）。
    let program = parse_src(
        "fn even(n: i32): bool {\n    return is_zero(n)\n}\nfn is_zero(n: i32): bool {\n    return n == 0\n}",
    );
    resolve(&program).expect("相互参照できるはず");
}

#[test]
fn reports_undefined_name() {
    let program = parse_src("fn f() {\n    return y\n}");
    let errs = resolve(&program).expect_err("未定義の名前でエラーになるはず");
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("未定義の名前"));
    assert!(errs[0].message.contains("y"));
}

#[test]
fn reports_duplicate_function() {
    let program = parse_src("fn f() {\n}\nfn f() {\n}");
    let errs = resolve(&program).expect_err("関数名重複でエラーになるはず");
    assert!(errs.iter().any(|e| e.message.contains("二重に定義")));
}

#[test]
fn reports_duplicate_param() {
    let program = parse_src("fn f(a: i32, a: i32) {\n}");
    let errs = resolve(&program).expect_err("引数名重複でエラーになるはず");
    assert!(errs.iter().any(|e| e.message.contains("引数")));
}

#[test]
fn allows_local_shadowing() {
    // 同一スコープでの let 再宣言（シャドーイング）は許可する。
    let program = parse_src("fn f() {\n    let x = 1\n    let x = 2\n    return x\n}");
    resolve(&program).expect("シャドーイングは許可されるはず");
}

#[test]
fn local_out_of_scope_after_block() {
    // if ブロック内のローカルはブロック外で見えない。
    let program = parse_src("fn f(c: bool) {\n    if c {\n        let inner = 1\n    }\n    return inner\n}");
    let errs = resolve(&program).expect_err("ブロック外で inner は未定義のはず");
    assert!(errs.iter().any(|e| e.message.contains("inner")));
}

#[test]
fn collects_multiple_errors() {
    let program = parse_src("fn f() {\n    return a + b\n}");
    let errs = resolve(&program).expect_err("複数の未定義エラーになるはず");
    assert_eq!(errs.len(), 2);
}

#[test]
fn def_kinds_are_recorded() {
    let program = parse_src("fn f(p: i32) {\n    let l = p\n}");
    let res = resolve(&program).expect("成功するはず");
    let kinds: Vec<_> = res.defs.iter().map(|d| d.kind).collect();
    assert!(kinds.contains(&DefKind::Function));
    assert!(kinds.contains(&DefKind::Param));
    assert!(kinds.contains(&DefKind::Local));
}
