//! モジュールシステムの回帰テスト。
//!
//! `use` 宣言、モジュールファイルの読み込み、pub 可視性、
//! モジュールパス呼び出し `mod.func(...)` を検証する。

use iris_lang::{compile, compile_ir};

/// テスト用のソースファイルが置かれたディレクトリのパスを返す。
fn fixture_path(name: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{manifest}/tests/fixtures/{name}")
}

/// `compile` の `name` 引数にフィクスチャのパスを使うことで、
/// モジュール解決の基準ディレクトリがそのディレクトリになる。
fn compile_with_fixture_base(src: &str, base_fixture: &str) -> Result<iris_lang::ast::Program, miette::Report> {
    let name = fixture_path(base_fixture);
    compile(&name, src)
}

fn compile_ir_with_fixture_base(src: &str, base_fixture: &str) -> Result<String, miette::Report> {
    let name = fixture_path(base_fixture);
    compile_ir(&name, src)
}

// ── パース・名前解決レベルのテスト ──────────────────────────────────────────

#[test]
fn test_use_missing_module_errors() {
    // 存在しないモジュールを use するとエラーになることを確認する。
    let src = r#"
use nonexistent_module.*
fn main(): i32 { 0 }
"#;
    let result = compile_with_fixture_base(src, "main.iris");
    assert!(
        result.is_err(),
        "存在しないモジュールはエラーになるはず"
    );
    let msg = format!("{:?}", result.unwrap_err());
    assert!(
        msg.contains("nonexistent_module") || msg.contains("読み込め"),
        "エラーメッセージにモジュール名が含まれるはず: {msg}"
    );
}

#[test]
fn test_use_plain_parses() {
    // `use mymath`（Plain import）が正しくパース・コンパイルされることを確認する。
    let src = r#"
use mymath
fn main(): i32 { mymath.add(1, 2) }
"#;
    compile_with_fixture_base(src, "main.iris")
        .expect("Plain import + モジュールパスアクセスが動くはず");
}

#[test]
fn test_use_glob_loads_module() {
    // `use mymath.*` で pub 関数が使えることを確認する。
    let src = r#"
use mymath.*
fn main(): i32 { add(1, 2) }
"#;
    compile_with_fixture_base(src, "main.iris").expect("グロブインポートで add が使えるはず");
}

#[test]
fn test_use_named_loads_functions() {
    // `use mymath { add }` で選択インポートできることを確認する。
    let src = r#"
use mymath { add }
fn main(): i32 { add(3, 4) }
"#;
    compile_with_fixture_base(src, "main.iris").expect("選択インポートで add が使えるはず");
}

#[test]
fn test_use_plain_module_path_access() {
    // `use mymath` + `mymath.add(...)` でアクセスできることを確認する。
    let src = r#"
use mymath
fn main(): i32 { mymath.add(1, 2) }
"#;
    compile_with_fixture_base(src, "main.iris")
        .expect("Plain import + モジュールパスアクセスが動くはず");
}

#[test]
fn test_use_prelude_is_noop() {
    // `use std.prelude.*` は自動前置済みのため重複エラーにならない。
    let src = r#"
use std.prelude.*
fn main(): i32 { 0 }
"#;
    compile_with_fixture_base(src, "main.iris").expect("use std.prelude.* は no-op になるはず");
}

// ── コード生成レベルのテスト ──────────────────────────────────────────────

#[test]
fn test_module_glob_codegen() {
    // グロブインポートした関数を使ったプログラムが IR を生成できることを確認する。
    let src = r#"
use mymath.*
fn main(): i32 {
    add(10, 32)
}
"#;
    let ir = compile_ir_with_fixture_base(src, "main.iris")
        .expect("IR 生成が成功するはず");
    // add 関数が IR に含まれることを確認する。
    assert!(ir.contains("@add"), "add 関数が IR に含まれるはず:\n{ir}");
    assert!(ir.contains("@main"), "main 関数が IR に含まれるはず:\n{ir}");
}

#[test]
fn test_module_path_call_codegen() {
    // `mymath.add(...)` 形式の呼び出しが正しい IR を生成することを確認する。
    let src = r#"
use mymath
fn main(): i32 {
    mymath.add(3, 4)
}
"#;
    let ir = compile_ir_with_fixture_base(src, "main.iris")
        .expect("モジュールパス呼び出しの IR 生成が成功するはず");
    assert!(ir.contains("@add"), "add への call が IR に含まれるはず:\n{ir}");
    assert!(ir.contains("@main"), "main 関数が IR に含まれるはず:\n{ir}");
}

/// clang が利用可能なときだけ実行ファイルまで検証する。
#[cfg(feature = "clang_tests")]
#[test]
fn test_module_glob_runs() {
    use std::process::Command;
    let src = r#"
use mymath.*
fn main(): i32 { add(10, 32) }
"#;
    let ir = compile_ir_with_fixture_base(src, "main.iris").expect("IR 生成");
    let ll = std::env::temp_dir().join("iris_module_test.ll");
    let exe = std::env::temp_dir().join("iris_module_test");
    std::fs::write(&ll, &ir).unwrap();
    let status = Command::new("clang")
        .args(["-O0", "-Wno-override-module", ll.to_str().unwrap(), "-o", exe.to_str().unwrap()])
        .status().unwrap();
    assert!(status.success(), "clang が成功するはず");
    let exit = Command::new(&exe).status().unwrap();
    assert_eq!(exit.code().unwrap(), 42, "add(10, 32) = 42 が戻り値になるはず");
    let _ = std::fs::remove_file(&ll);
    let _ = std::fs::remove_file(&exe);
}
