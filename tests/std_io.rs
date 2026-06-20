//! 標準ライブラリ（自動前置）を使って実際に出力するプログラムの実行テスト。
//! `clang` が無い環境ではスキップする。

use std::process::Command;

fn clang_available() -> bool {
    Command::new("clang")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// IR を clang でビルドして実行し、(標準出力, 終了コード) を返す。clang が無ければ None。
fn run(src: &str, tag: &str) -> Option<(String, i32)> {
    if !clang_available() {
        return None;
    }
    let ir = iris_lang::compile_ir("test", src).expect("コード生成に成功するはず");
    let dir = std::env::temp_dir();
    let ll = dir.join(format!("iris_std_{tag}.ll"));
    let exe = dir.join(format!("iris_std_{tag}.bin"));
    std::fs::write(&ll, ir).unwrap();
    let build = Command::new("clang")
        .arg(&ll)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("clang");
    assert!(
        build.status.success(),
        "clang 失敗:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let out = Command::new(&exe).output().expect("実行");
    let _ = std::fs::remove_file(&ll);
    let _ = std::fs::remove_file(&exe);
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    ))
}

#[test]
fn println_int_outputs_number() {
    let src = "fn main(): i32 {\n    let r = println_int(12345)\n    return 0\n}";
    if let Some((stdout, code)) = run(src, "num") {
        assert_eq!(stdout, "12345\n");
        assert_eq!(code, 0);
    }
}

#[test]
fn println_int_handles_negative() {
    let src = "fn main(): i32 {\n    let r = println_int(0 - 42)\n    return 0\n}";
    if let Some((stdout, _)) = run(src, "neg") {
        assert_eq!(stdout, "-42\n");
    }
}

#[test]
fn computes_and_prints_fib() {
    let src = "fn fib(n: i32): i32 {\n    if n < 2 {\n        return n\n    }\n    return fib(n - 1) + fib(n - 2)\n}\nfn main(): i32 {\n    let r = println_int(fib(10))\n    return 0\n}";
    if let Some((stdout, _)) = run(src, "fib") {
        assert_eq!(stdout, "55\n");
    }
}

#[test]
fn println_bool_outputs_one_zero() {
    let src = "fn main(): i32 {\n    let a = println_bool(2 > 1)\n    let b = println_bool(1 > 2)\n    return 0\n}";
    if let Some((stdout, _)) = run(src, "bool") {
        assert_eq!(stdout, "1\n0\n");
    }
}

#[test]
fn user_can_call_putchar_directly() {
    // extern fn putchar を直接使える（'A','B','改行'）。
    let src = "fn main(): i32 {\n    let a = putchar(65)\n    let b = putchar(66)\n    let c = putchar(10)\n    return 0\n}";
    if let Some((stdout, _)) = run(src, "putchar") {
        assert_eq!(stdout, "AB\n");
    }
}
