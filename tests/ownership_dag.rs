//! 所有権 DAG（型レベルの循環検出）とライフタイム（借用グラフ）の回帰テスト。

use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::sema::{check, check_ownership, resolve};

fn ownck(src: &str) -> Result<(), Vec<String>> {
    let prog = {
        let tokens = lex(src).expect("字句解析");
        parse(&tokens).expect("構文解析")
    };
    let resolution = resolve(&prog, Default::default()).expect("名前解決");
    let type_info = check(&prog, &resolution).expect("型検査");
    check_ownership(&prog, &resolution, &type_info)
        .map_err(|errs| errs.into_iter().map(|e| e.message).collect())
}

// ---- 型レベルの所有権 DAG（循環検出） ----------------------------------

#[test]
fn rejects_direct_ownership_cycle() {
    let errs = ownck("type Bad = struct {\n    me: Bad\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("所有が循環")));
}

#[test]
fn rejects_mutual_ownership_cycle() {
    let errs = ownck("type A = struct {\n    b: B\n}\ntype B = struct {\n    a: A\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("所有が循環")));
}

#[test]
fn reference_breaks_cycle() {
    // 自己参照は `&` なら非所有なので循環しない。
    ownck("type Node = struct {\n    parent: &Node\n}").expect("参照は所有辺を作らない");
}

#[test]
fn heap_indirection_breaks_cycle() {
    // Vec / Box はヒープ間接なので循環しない。
    ownck("type Tree = struct {\n    kids: Vec<Tree>\n}").expect("Vec は所有辺を作らない");
    ownck("type List = struct {\n    next: Box<List>\n}").expect("Box は所有辺を作らない");
}

#[test]
fn enum_payload_cycle_detected() {
    // enum のペイロードがインライン所有で循環する場合。
    let errs = ownck("type E = enum { Node: E }").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("所有が循環")));
}

#[test]
fn tuple_inline_cycle_detected() {
    let errs = ownck("type T = struct {\n    pair: (T, i32)\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("所有が循環")));
}

// ---- ライフタイム（借用グラフ） ----------------------------------------

#[test]
fn rejects_returning_ref_to_local() {
    let errs = ownck("fn dangle(): &i32 {\n    let x = 1\n    return &x\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("ローカルな値 `x` を指す参照を返")));
}

#[test]
fn rejects_returning_ref_to_value_param() {
    // 値渡し引数は関数が所有するので、その参照を返すとダングリング。
    let errs = ownck("fn dangle(p: i32): &i32 {\n    return &p\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("参照を返")));
}

#[test]
fn allows_returning_ref_param() {
    // 参照引数の値を返すのは安全（呼び出し側が所有）。
    ownck("fn id(p: &i32): &i32 {\n    return p\n}").expect("参照引数の返却は安全");
}

#[test]
fn allows_returning_ref_through_local_binding() {
    // ローカルに束ねた参照引数を返すのも安全（provenance が呼び出し側へ伝播）。
    ownck("fn id(p: &i32): &i32 {\n    let r = p\n    return r\n}").expect("provenance 伝播");
}

#[test]
fn rejects_use_of_ref_after_referent_moved() {
    let src = "type P = struct {\n    x: i32\n}\nfn take(p: P): i32 {\n    return p.x\n}\nfn peek(r: &P): i32 {\n    return r.x\n}\nfn run(): i32 {\n    let a = P { x: 1 }\n    let r = &a\n    let b = take(a)\n    return peek(r)\n}";
    let errs = ownck(src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("ムーブされた値 `a` を指す参照 `r`")));
}

#[test]
fn example_hello_passes() {
    let src = include_str!("../examples/hello.iris");
    ownck(src).expect("サンプルは所有権検査を通る");
}
