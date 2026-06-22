//! 借用競合チェック（エイリアス規則）の回帰テスト。

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

const P: &str = "type P = struct {\n    x: i32\n}\n";

#[test]
fn rejects_mut_and_shared() {
    let src = format!("{P}fn use2(a: &mut P, b: &P): i32 {{\n    return b.x\n}}\nfn run(): i32 {{\n    let mut p = P {{ x: 1 }}\n    let r = &mut p\n    let s = &p\n    return use2(r, s)\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("可変借用と共有借用")));
}

#[test]
fn rejects_double_mut() {
    let src = format!("{P}fn run() {{\n    let mut p = P {{ x: 1 }}\n    let r = &mut p\n    let s = &mut p\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("二重で可変借用")));
}

#[test]
fn allows_multiple_shared() {
    let src = format!("{P}fn run() {{\n    let p = P {{ x: 1 }}\n    let r = &p\n    let s = &p\n}}");
    ownck(&src).expect("共有借用は複数可");
}

#[test]
fn rejects_conflict_within_one_call() {
    let src = format!("{P}fn use2(a: &mut P, b: &P): i32 {{\n    return b.x\n}}\nfn run(): i32 {{\n    let mut p = P {{ x: 1 }}\n    return use2(&mut p, &p)\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("可変借用と共有借用")));
}

#[test]
fn allows_double_mut_in_separate_scopes() {
    // 別ブロックで作られた借用は競合しない。
    let src = format!("{P}fn run(c: bool) {{\n    let mut p = P {{ x: 1 }}\n    if c {{\n        let r = &mut p\n    }}\n    let s = &mut p\n}}");
    ownck(&src).expect("スコープが分かれていれば競合しない");
}

#[test]
fn different_places_do_not_conflict() {
    let src = format!("{P}fn run() {{\n    let mut p = P {{ x: 1 }}\n    let mut q = P {{ x: 2 }}\n    let r = &mut p\n    let s = &mut q\n}}");
    ownck(&src).expect("別の場所の借用は競合しない");
}

#[test]
fn reassign_releases_old_borrow() {
    // 借用変数への再代入は古い借用を解放する。
    let src = format!("{P}fn run() {{\n    let mut p = P {{ x: 1 }}\n    let mut r = &p\n    r = &p\n    let s = &p\n}}");
    ownck(&src).expect("共有借用の再代入は競合しない");
}

#[test]
fn write_through_keeps_borrow_alive() {
    // 参照越し代入（write-through, `r = P{..}`）は r の借用を解放しない。
    // よって r が p を可変借用したまま再度 `&mut p` すると競合する。
    let src = format!(
        "{P}fn run() {{\n    let mut p = P {{ x: 1 }}\n    let mut r = &mut p\n    r = P {{ x: 5 }}\n    let s = &mut p\n}}"
    );
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("二重で可変借用")));
}

#[test]
fn shared_then_mut_conflicts() {
    let src = format!("{P}fn use2(a: &P, b: &mut P): i32 {{\n    return a.x\n}}\nfn run(): i32 {{\n    let mut p = P {{ x: 1 }}\n    let r = &p\n    let s = &mut p\n    return use2(r, s)\n}}");
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("可変借用と共有借用")));
}

#[test]
fn example_hello_passes() {
    let src = include_str!("../examples/hello.iris");
    ownck(src).expect("サンプルは所有権検査を通る");
}
