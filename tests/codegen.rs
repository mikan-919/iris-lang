//! LLVM IR コード生成の回帰テスト。
//!
//! IR テキストの検査は常に実行する。`clang` が使える環境では、生成した IR を
//! 実行ファイルにして終了コードまで検証する（無い環境ではスキップ）。

use std::process::Command;

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
    let ll = dir.join(format!("iris_cg_{tag}.ll"));
    let exe = dir.join(format!("iris_cg_{tag}.bin"));
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

#[test]
fn emits_function_and_recursion() {
    let ir = emit("fn fac(n: i32): i32 {\n    if n <= 1 {\n        return 1\n    }\n    return n * fac(n - 1)\n}");
    assert!(ir.contains("define i32 @fac(i32 %arg0)"));
    assert!(ir.contains("call i32 @fac"));
    assert!(ir.contains("icmp sle i32"));
    assert!(ir.contains("mul i32"));
}

#[test]
fn emits_short_circuit_for_logical_and() {
    // && は分岐（短絡）で生成される。
    let ir = emit("fn f(a: bool, b: bool): bool {\n    return a && b\n}");
    assert!(ir.contains("br i1"));
    assert!(ir.contains("alloca i1"));
}

#[test]
fn unsupported_type_is_reported() {
    // f64 はまだ未対応。
    let err = iris_lang::compile_ir("test", "fn f(): f64 {\n    return 1.0\n}");
    assert!(err.is_err());
}

#[test]
fn runs_factorial() {
    let src = "fn fac(n: i32): i32 {\n    if n <= 1 {\n        return 1\n    }\n    return n * fac(n - 1)\n}\nfn main(): i32 {\n    return fac(5)\n}";
    if let Some(code) = run_exit_code(src, "fac") {
        assert_eq!(code, 120);
    }
}

#[test]
fn runs_arithmetic_and_let() {
    let src = "fn main(): i32 {\n    let a = 6\n    let mut b = 7\n    b = b * a\n    return b - 2\n}";
    if let Some(code) = run_exit_code(src, "arith") {
        assert_eq!(code, 40);
    }
}

#[test]
fn runs_if_else_and_comparison() {
    let src = "fn classify(n: i32): i32 {\n    if n > 10 {\n        return 2\n    } else {\n        return 1\n    }\n}\nfn main(): i32 {\n    return classify(5) + classify(20)\n}";
    if let Some(code) = run_exit_code(src, "ifelse") {
        assert_eq!(code, 3);
    }
}

#[test]
fn runs_ternary_and_short_circuit() {
    let src = "fn main(): i32 {\n    let x = 5\n    let ok = x > 0 && x < 10\n    return ok ? x * 2 : 0\n}";
    if let Some(code) = run_exit_code(src, "tern") {
        assert_eq!(code, 10);
    }
}

#[test]
fn emits_while_loop_blocks() {
    let ir = emit("fn main(): i32 {\n    let mut i = 0\n    while i < 3 {\n        i = i + 1\n    }\n    return i\n}");
    assert!(ir.contains("while.cond"));
    assert!(ir.contains("while.body"));
    assert!(ir.contains("while.end"));
    // 条件への分岐（後方辺）と本体/末尾への条件分岐。
    assert!(ir.contains("br i1"));
}

#[test]
fn runs_while_sum() {
    // 1..=5 の合計 = 15。
    let src = "fn main(): i32 {\n    let mut i = 1\n    let mut acc = 0\n    while i <= 5 {\n        acc = acc + i\n        i = i + 1\n    }\n    return acc\n}";
    if let Some(code) = run_exit_code(src, "while_sum") {
        assert_eq!(code, 15);
    }
}

#[test]
fn runs_loop_with_break() {
    // 100 を超えない最大の 2 のべき乗 = 64。
    let src = "fn main(): i32 {\n    let mut x = 1\n    loop {\n        if x * 2 > 100 {\n            break\n        }\n        x = x * 2\n    }\n    return x\n}";
    if let Some(code) = run_exit_code(src, "loop_break") {
        assert_eq!(code, 64);
    }
}

#[test]
fn runs_while_with_continue() {
    // 1..=10 のうち偶数の合計 = 30。
    let src = "fn main(): i32 {\n    let mut i = 0\n    let mut acc = 0\n    while i < 10 {\n        i = i + 1\n        if i % 2 == 1 {\n            continue\n        }\n        acc = acc + i\n    }\n    return acc\n}";
    if let Some(code) = run_exit_code(src, "while_cont") {
        assert_eq!(code, 30);
    }
}
