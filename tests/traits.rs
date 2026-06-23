//! トレイトシステム（ADR-0004〜0009）の回帰テスト。
//!
//! `trait` 定義 / `impl Trait for Type` / 適合検査 / メソッド解決（`#` 修飾子・曖昧性）/
//! 既定実装 / スーパートレイト / ジェネリック境界 `<T: Bound>` と単相化 / 構造的 `==` を、
//! 構文解析・型検査・所有権検査・コード生成（clang があれば実行）まで縦断して検証する。

use std::process::Command;

use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::sema::{check, check_ownership, resolve};

/// 型検査まで通す。エラーがあればメッセージ一覧を返す。
fn typecheck(src: &str) -> Result<(), Vec<String>> {
    let tokens = lex(src).expect("字句解析");
    let program = parse(&tokens).expect("構文解析");
    let resolution = resolve(&program, Default::default()).expect("名前解決");
    check(&program, &resolution)
        .map(|_| ())
        .map_err(|errs| errs.into_iter().map(|e| e.message).collect())
}

/// 名前解決・型検査・所有権検査まで通す。所有権エラーのメッセージ一覧を返す。
fn ownck(src: &str) -> Result<(), Vec<String>> {
    let tokens = lex(src).expect("字句解析");
    let program = parse(&tokens).expect("構文解析");
    let resolution = resolve(&program, Default::default()).expect("名前解決");
    let type_info = check(&program, &resolution).expect("型検査");
    check_ownership(&program, &resolution, &type_info)
        .map_err(|errs| errs.into_iter().map(|e| e.message).collect())
}

fn clang_available() -> bool {
    Command::new("clang")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// IR を clang でコンパイルして実行し、終了コードを返す。clang が無ければ None。
fn run_exit_code(src: &str, tag: &str) -> Option<i32> {
    if !clang_available() {
        return None;
    }
    let ir = iris_lang::compile_ir("test", src).expect("コード生成に成功するはず");
    let dir = std::env::temp_dir();
    let ll = dir.join(format!("iris_trait_{tag}.ll"));
    let exe = dir.join(format!("iris_trait_{tag}.bin"));
    std::fs::write(&ll, ir).unwrap();
    let status = Command::new("clang")
        .arg("-nostartfiles")
        .arg(&ll)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("clang 実行");
    assert!(
        status.status.success(),
        "clang コンパイル失敗:\n{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let run = Command::new(&exe).status().expect("実行");
    let _ = std::fs::remove_file(&ll);
    let _ = std::fs::remove_file(&exe);
    run.code()
}

const POINT: &str = "type Point = struct {\n    x: i32\n    y: i32\n}\n";

// ---- 構文解析 -----------------------------------------------------------

#[test]
fn parses_trait_and_impl_for() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n}}\nimpl Greet for Point {{\n    fn hello(self): i32 {{\n        return self.x + self.y\n    }}\n}}\n"
    );
    let tokens = lex(&src).expect("字句解析");
    let program = parse(&tokens).expect("trait / impl Trait for が解析できるはず");
    use iris_lang::ast::Item;
    let traits = program
        .items
        .iter()
        .filter(|i| matches!(i, Item::Trait(_)))
        .count();
    let impls = program
        .items
        .iter()
        .filter(|i| matches!(i, Item::Impl(_)))
        .count();
    assert_eq!(traits, 1);
    assert_eq!(impls, 1);
}

#[test]
fn parses_generic_trait_supertrait_and_bounds() {
    let src = "trait Eq2 {\n    fn same(self): i32\n}\ntrait Ord2: Eq2 {\n    fn cmp(self): i32\n}\ntrait Iter<T> {\n    fn next(&mut self): i32\n}\nfn use_it<T: Ord2>(x: T): i32 {\n    return x.cmp()\n}\n";
    let tokens = lex(src).expect("字句解析");
    parse(&tokens).expect("ジェネリックトレイト・スーパートレイト・境界が解析できるはず");
}

// ---- 型検査（適合） -----------------------------------------------------

#[test]
fn conformance_ok() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n}}\nimpl Greet for Point {{\n    fn hello(self): i32 {{\n        return self.x\n    }}\n}}\n"
    );
    assert!(typecheck(&src).is_ok());
}

#[test]
fn conformance_missing_method_errors() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n    fn bye(self): i32\n}}\nimpl Greet for Point {{\n    fn hello(self): i32 {{\n        return self.x\n    }}\n}}\n"
    );
    let errs = typecheck(&src).expect_err("bye が未実装でエラー");
    assert!(errs.iter().any(|e| e.contains("bye")), "{errs:?}");
}

#[test]
fn conformance_signature_mismatch_errors() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n}}\nimpl Greet for Point {{\n    fn hello(self): bool {{\n        return true\n    }}\n}}\n"
    );
    let errs = typecheck(&src).expect_err("戻り値型不一致でエラー");
    assert!(errs.iter().any(|e| e.contains("シグネチャ")), "{errs:?}");
}

#[test]
fn conformance_unknown_method_errors() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n}}\nimpl Greet for Point {{\n    fn hello(self): i32 {{\n        return self.x\n    }}\n    fn extra(self): i32 {{\n        return 0\n    }}\n}}\n"
    );
    let errs = typecheck(&src).expect_err("トレイトに無い extra でエラー");
    assert!(errs.iter().any(|e| e.contains("extra")), "{errs:?}");
}

#[test]
fn supertrait_requires_impl() {
    // Ord2 は Eq2 を要求する。Point が Eq2 を実装していなければエラー。
    let src = format!(
        "{POINT}trait Eq2 {{\n    fn same(self): i32\n}}\ntrait Ord2: Eq2 {{\n    fn cmp(self): i32\n}}\nimpl Ord2 for Point {{\n    fn cmp(self): i32 {{\n        return self.x\n    }}\n}}\n"
    );
    let errs = typecheck(&src).expect_err("スーパートレイト Eq2 未実装でエラー");
    assert!(errs.iter().any(|e| e.contains("Eq2")), "{errs:?}");
}

#[test]
fn duplicate_impl_errors() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n}}\nimpl Greet for Point {{\n    fn hello(self): i32 {{\n        return self.x\n    }}\n}}\nimpl Greet for Point {{\n    fn hello(self): i32 {{\n        return self.y\n    }}\n}}\n"
    );
    let errs = typecheck(&src).expect_err("同一キーの重複実装でエラー");
    assert!(errs.iter().any(|e| e.contains("重複")), "{errs:?}");
}

// ---- 型検査（メソッド解決・境界） ---------------------------------------

#[test]
fn ambiguous_method_requires_hash() {
    // 固有 hello と trait hello が衝突 → 修飾なしは曖昧（ADR-0004 D2）。
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(&self): i32\n}}\nimpl Point {{\n    fn hello(&self): i32 {{\n        return self.x\n    }}\n}}\nimpl Greet for Point {{\n    fn hello(&self): i32 {{\n        return self.y\n    }}\n}}\nfn main(): i32 {{\n    let p = Point {{ x: 1, y: 2 }}\n    return p.hello()\n}}\n"
    );
    let errs = typecheck(&src).expect_err("曖昧でエラー");
    assert!(errs.iter().any(|e| e.contains("提供元が複数")), "{errs:?}");
}

#[test]
fn unsatisfied_bound_errors() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n}}\nfn run<T: Greet>(x: T): i32 {{\n    return x.hello()\n}}\nfn main(): i32 {{\n    let p = Point {{ x: 1, y: 2 }}\n    return run(p)\n}}\n"
    );
    let errs = typecheck(&src).expect_err("Point は Greet 未実装で境界違反");
    assert!(errs.iter().any(|e| e.contains("境界")), "{errs:?}");
}

// ---- 所有権 -------------------------------------------------------------

#[test]
fn struct_equality_does_not_move() {
    // `==` は読みのみで被演算子を消費しない（ADR-0009）。
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let a = Point {{ x: 1, y: 2 }}\n    let b = Point {{ x: 1, y: 2 }}\n    let e1 = a == b\n    let e2 = a == b\n    return 0\n}}\n"
    );
    assert!(ownck(&src).is_ok(), "{:?}", ownck(&src));
}

// ---- コード生成・実行 ---------------------------------------------------

#[test]
fn runs_concrete_trait_dispatch() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n}}\nimpl Greet for Point {{\n    fn hello(self): i32 {{\n        return self.x + self.y\n    }}\n}}\nfn main(): i32 {{\n    let p = Point {{ x: 3, y: 4 }}\n    return p.hello()\n}}\n"
    );
    if let Some(code) = run_exit_code(&src, "concrete") {
        assert_eq!(code, 7);
    }
}

#[test]
fn runs_default_method() {
    let src = "type Counter = struct {\n    n: i32\n}\ntrait Describe {\n    fn value(self): i32\n    fn doubled(self): i32 {\n        return self.value() + self.value()\n    }\n}\nimpl Describe for Counter {\n    fn value(self): i32 {\n        return self.n\n    }\n}\nfn main(): i32 {\n    let c = Counter { n: 21 }\n    return c.doubled()\n}\n";
    if let Some(code) = run_exit_code(src, "default") {
        assert_eq!(code, 42);
    }
}

#[test]
fn runs_generic_monomorphization() {
    let src = format!(
        "{POINT}trait Greet {{\n    fn hello(self): i32\n}}\nimpl Greet for Point {{\n    fn hello(self): i32 {{\n        return self.x + self.y\n    }}\n}}\nfn greet_all<T: Greet>(v: T): i32 {{\n    return v.hello()\n}}\nfn main(): i32 {{\n    let p = Point {{ x: 3, y: 4 }}\n    return greet_all(p)\n}}\n"
    );
    if let Some(code) = run_exit_code(&src, "mono") {
        assert_eq!(code, 7);
    }
}

#[test]
fn runs_hash_qualifier_collision() {
    // 固有 show と trait show を `#` で指し分ける（別記号に分離されること）。
    let src = "type Widget = struct {\n    id: i32\n}\ntrait Render {\n    fn show(&self): i32\n}\nimpl Widget {\n    fn show(&self): i32 {\n        return self.id\n    }\n}\nimpl Render for Widget {\n    fn show(&self): i32 {\n        return self.id + 100\n    }\n}\nfn main(): i32 {\n    let w = Widget { id: 5 }\n    return w.show#Render() + w.show#Widget()\n}\n";
    if let Some(code) = run_exit_code(src, "hash") {
        assert_eq!(code, 110);
    }
}

#[test]
fn runs_structural_equality() {
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\nfn main(): i32 {\n    let a = Point { x: 1, y: 2 }\n    let b = Point { x: 1, y: 2 }\n    let c = Point { x: 1, y: 9 }\n    let eq = a == b\n    let ne = a != c\n    return (eq ? 1 : 0) + (ne ? 100 : 0)\n}\n";
    if let Some(code) = run_exit_code(src, "structeq") {
        assert_eq!(code, 101);
    }
}

#[test]
fn runs_generic_through_supertrait() {
    // `<T: Sub>` から Super のメソッドを呼べる（ADR-0007: 境界はスーパートレイトを含意）。
    let src = "type Box1 = struct {\n    v: i32\n}\ntrait Base {\n    fn base(self): i32\n}\ntrait Ext: Base {\n    fn ext(self): i32\n}\nimpl Base for Box1 {\n    fn base(self): i32 {\n        return self.v\n    }\n}\nimpl Ext for Box1 {\n    fn ext(self): i32 {\n        return self.v\n    }\n}\nfn run<T: Ext>(x: T): i32 {\n    return x.base()\n}\nfn main(): i32 {\n    let b = Box1 { v: 9 }\n    return run(b)\n}\n";
    if let Some(code) = run_exit_code(src, "super") {
        assert_eq!(code, 9);
    }
}
