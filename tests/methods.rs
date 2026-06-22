//! 固有メソッド `impl Type { fn ... }` とドット呼び出し `x.m()` の回帰テスト。
//!
//! self の三形（`self` / `&self` / `&mut self`）を、構文解析・型検査・所有権検査・
//! コード生成（clang があれば実行）まで縦断して検証する。

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

fn emit(src: &str) -> String {
    iris_lang::compile_ir("test", src).expect("コード生成に成功するはず")
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
    let ir = emit(src);
    let dir = std::env::temp_dir();
    let ll = dir.join(format!("iris_method_{tag}.ll"));
    let exe = dir.join(format!("iris_method_{tag}.bin"));
    std::fs::write(&ll, ir).unwrap();
    let status = Command::new("clang")
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

/// self の三形を持つ Point の定義。
const POINT: &str = "type Point = struct {\n    x: i32\n    y: i32\n}\nimpl Point {\n    fn into_sum(self): i32 {\n        return self.x + self.y\n    }\n    fn scaled(&self, k: i32): i32 {\n        return (self.x + self.y) * k\n    }\n    fn shift(&mut self, d: i32): i32 {\n        self.x = self.x + d\n        self.y = self.y + d\n        return 0\n    }\n}\n";

// ---- 構文解析 -----------------------------------------------------------

#[test]
fn parses_impl_block_with_self_forms() {
    let tokens = lex(POINT).expect("字句解析");
    let program = parse(&tokens).expect("impl ブロックが解析できるはず");
    let impls = program
        .items
        .iter()
        .filter(|i| matches!(i, iris_lang::ast::Item::Impl(_)))
        .count();
    assert_eq!(impls, 1);
}

// ---- 型検査 -------------------------------------------------------------

#[test]
fn typechecks_method_dispatch() {
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let mut p = Point {{ x: 3, y: 4 }}\n    let a = p.scaled(2)\n    let b = p.shift(1)\n    let c = p.into_sum()\n    return a + b + c\n}}"
    );
    typecheck(&src).expect("メソッド呼び出しが型整合するはず");
}

#[test]
fn rejects_unknown_method() {
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let p = Point {{ x: 1, y: 2 }}\n    return p.nope()\n}}"
    );
    let errs = typecheck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("メソッド `nope` はありません")));
}

#[test]
fn rejects_mut_method_on_immutable_binding() {
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let p = Point {{ x: 1, y: 2 }}\n    return p.shift(1)\n}}"
    );
    let errs = typecheck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("可変メソッド `shift`")));
}

#[test]
fn rejects_method_arg_count_mismatch() {
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let p = Point {{ x: 1, y: 2 }}\n    return p.scaled(1, 2)\n}}"
    );
    let errs = typecheck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("引数は 1 個")));
}

#[test]
fn rejects_method_arg_type_mismatch() {
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let p = Point {{ x: 1, y: 2 }}\n    return p.scaled(true)\n}}"
    );
    let errs = typecheck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("引数の型が一致しません")));
}

#[test]
fn rejects_duplicate_method_same_impl() {
    let src = "type P = struct {\n    x: i32\n}\nimpl P {\n    fn m(&self): i32 { return self.x }\n    fn m(&self): i32 { return self.x }\n}\nfn main(): i32 {\n    let p = P { x: 1 }\n    return p.m()\n}";
    let errs = typecheck(src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("メソッド `m` が二重に定義")));
}

#[test]
fn rejects_duplicate_method_across_impls() {
    // 別々の impl ブロックに分かれていても同じ型の同名メソッドは二重定義。
    let src = "type P = struct {\n    x: i32\n}\nimpl P {\n    fn m(&self): i32 { return self.x }\n}\nimpl P {\n    fn m(&self): i32 { return self.x }\n}\nfn main(): i32 {\n    let p = P { x: 1 }\n    return p.m()\n}";
    let errs = typecheck(src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("二重に定義")));
}

#[test]
fn same_method_name_on_different_types_is_ok() {
    // 別の型の同名メソッドは衝突しない。
    let src = "type A = struct {\n    v: i32\n}\ntype B = struct {\n    v: i32\n}\nimpl A {\n    fn get(&self): i32 { return self.v }\n}\nimpl B {\n    fn get(&self): i32 { return self.v }\n}\nfn main(): i32 {\n    let a = A { v: 1 }\n    let b = B { v: 2 }\n    return a.get() + b.get()\n}";
    typecheck(src).expect("別の型の同名メソッドは許可されるはず");
}

// ---- 所有権 -------------------------------------------------------------

#[test]
fn value_self_moves_receiver() {
    // value self メソッドは受け手をムーブする。以降の使用はエラー。
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let p = Point {{ x: 1, y: 2 }}\n    let a = p.into_sum()\n    let b = p.into_sum()\n    return a + b\n}}"
    );
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("ムーブ済みの値 `p`")));
}

#[test]
fn ref_self_does_not_move() {
    // &self メソッドは借用なのでムーブしない。
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let p = Point {{ x: 1, y: 2 }}\n    let a = p.scaled(1)\n    let b = p.scaled(2)\n    return a + b\n}}"
    );
    ownck(&src).expect("&self は受け手をムーブしないはず");
}

#[test]
fn mut_self_call_conflicts_with_live_borrow() {
    // 生存中の &mut 借用がある間に &mut self メソッドを呼ぶと競合。
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let mut p = Point {{ x: 1, y: 2 }}\n    let r = &mut p\n    let z = p.shift(1)\n    return 0\n}}"
    );
    let errs = ownck(&src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("可変借用")));
}

// ---- コード生成・実行 ---------------------------------------------------

#[test]
fn emits_mangled_method_symbols() {
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let mut p = Point {{ x: 3, y: 4 }}\n    let a = p.scaled(2)\n    let b = p.shift(10)\n    return a + p.into_sum()\n}}"
    );
    let ir = emit(&src);
    // メソッドは Type.method の記号で定義される。
    assert!(ir.contains("define i32 @Point.into_sum(%Point %arg0)"));
    assert!(ir.contains("define i32 @Point.scaled(ptr %arg0, i32 %arg1)"));
    assert!(ir.contains("define i32 @Point.shift(ptr %arg0, i32 %arg1)"));
    // 呼び出しも同じ記号。
    assert!(ir.contains("call i32 @Point.scaled"));
    assert!(ir.contains("call i32 @Point.into_sum"));
}

#[test]
fn runs_all_three_self_forms() {
    // p={3,4}; scaled(2)=14; shift(10) → p={13,14}; into_sum=27; 14+27=41
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let mut p = Point {{ x: 3, y: 4 }}\n    let t = p.scaled(2)\n    let z = p.shift(10)\n    let s = p.into_sum()\n    return t + s\n}}"
    );
    if let Some(code) = run_exit_code(&src, "self_forms") {
        assert_eq!(code, 41);
    }
}

#[test]
fn runs_mut_self_write_through() {
    // &mut self でフィールドを書き換え、その効果が呼び出し側に反映される。
    let src = format!(
        "{POINT}fn main(): i32 {{\n    let mut p = Point {{ x: 1, y: 2 }}\n    let z = p.shift(5)\n    return p.x + p.y\n}}"
    );
    // p={1,2} → shift(5) → {6,7} → 13
    if let Some(code) = run_exit_code(&src, "write_through") {
        assert_eq!(code, 13);
    }
}
