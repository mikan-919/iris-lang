//! 所有権（ムーブ）検査の回帰テスト。

use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::sema::{check, check_ownership, resolve};

/// 名前解決・型検査・所有権検査まで通す。所有権エラーのメッセージ一覧を返す。
fn ownck(src: &str) -> Result<(), Vec<String>> {
    let prog = {
        let tokens = lex(src).expect("字句解析");
        parse(&tokens).expect("構文解析")
    };
    let resolution = resolve(&prog).expect("名前解決");
    let type_info = check(&prog, &resolution).expect("型検査");
    check_ownership(&prog, &resolution, &type_info)
        .map_err(|errs| errs.into_iter().map(|e| e.message).collect())
}

const STRUCT: &str = "type P = struct {\n    x: i32\n}\nfn take(p: P): i32 {\n    return p.x\n}\n";

#[test]
fn rejects_use_after_move() {
    let src = format!("{STRUCT}fn run(): i32 {{\n    let a = P {{ x: 1 }}\n    let b = take(a)\n    let c = take(a)\n    return b + c\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("ムーブ済みの値 `a`")));
}

#[test]
fn copy_types_are_not_moved() {
    // i32 は Copy なので二度使ってもよい。
    ownck("fn f(): i32 {\n    let a = 1\n    let b = a\n    let c = a\n    return b + c\n}")
        .expect("Copy 型はムーブされない");
}

#[test]
fn mut_ref_deref_read_does_not_move() {
    // `&mut` は非 Copy だが、算術オペランド（auto-deref）での読みはムーブではない。
    // `r = r + 1` は右辺で r を読み、続けて write-through で r を使える。
    ownck("fn add_one(r: &mut i32) {\n    r = r + 1\n}").expect("auto-deref 読みはムーブしない");
}

#[test]
fn borrow_does_not_move() {
    let src = format!("{STRUCT}fn peek(p: &P): i32 {{\n    return p.x\n}}\nfn run(): i32 {{\n    let a = P {{ x: 1 }}\n    let b = peek(&a)\n    let c = take(a)\n    return b + c\n}}");
    ownck(&src).expect("借用はムーブしない");
}

#[test]
fn rejects_borrow_after_move() {
    let src = format!("{STRUCT}fn peek(p: &P): i32 {{\n    return p.x\n}}\nfn run(): i32 {{\n    let a = P {{ x: 1 }}\n    let b = take(a)\n    let c = peek(&a)\n    return b + c\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("ムーブ済みの値 `a` を借用")));
}

#[test]
fn reassignment_reinitializes() {
    // ムーブ後に再代入すれば再び使える。
    let src = format!("{STRUCT}fn run(): i32 {{\n    let mut a = P {{ x: 1 }}\n    let b = take(a)\n    a = P {{ x: 2 }}\n    let c = take(a)\n    return b + c\n}}");
    ownck(&src).expect("再代入で再初期化される");
}

#[test]
fn move_in_one_if_branch_is_conservative() {
    // 片方の分岐でムーブしたら、分岐後はムーブ済みとみなす。
    let src = format!("{STRUCT}fn run(c: bool): i32 {{\n    let a = P {{ x: 1 }}\n    if c {{\n        let b = take(a)\n    }}\n    let d = take(a)\n    return d\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("ムーブ済みの値 `a`")));
}

#[test]
fn alias_of_primitive_is_copy() {
    // type Meters = f64 は Copy。
    let src = "type Meters = f64\nfn run() {\n    let a: Meters = 1.0\n    let b = a\n    let c = a\n}";
    ownck(src).expect("プリミティブ別名は Copy");
}

#[test]
fn move_into_struct_literal_field() {
    let src = format!("type Q = struct {{\n    p: P\n}}\n{STRUCT}fn run(): i32 {{\n    let a = P {{ x: 1 }}\n    let q = Q {{ p: a }}\n    return take(a)\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("ムーブ済みの値 `a`")));
}

#[test]
fn rejects_move_across_loop_iterations() {
    // ループ末尾でムーブした値を次の反復の先頭で再びムーブ → use-after-move。
    let src = format!("{STRUCT}fn run(): i32 {{\n    let a = P {{ x: 1 }}\n    let mut n = 0\n    while n < 3 {{\n        let b = take(a)\n        n = n + 1\n    }}\n    return 0\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("ムーブ済みの値 `a`")));
}

#[test]
fn allows_per_iteration_fresh_value_move() {
    // 反復ごとに作り直す値のムーブは誤検出しない。
    let src = format!("{STRUCT}fn make(): P {{\n    return P {{ x: 1 }}\n}}\nfn run(): i32 {{\n    let mut n = 0\n    while n < 3 {{\n        let a = make()\n        let b = take(a)\n        n = n + 1\n    }}\n    return 0\n}}");
    ownck(&src).expect("反復ごとの新規値のムーブは正当");
}

#[test]
fn example_hello_passes_ownership() {
    let src = include_str!("../examples/hello.iris");
    ownck(src).expect("サンプルは所有権検査を通る");
}
