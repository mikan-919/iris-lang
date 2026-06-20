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
    // 文字列はまだコード生成に未対応。
    let err = iris_lang::compile_ir("test", "fn f(): string {\n    return \"hi\"\n}");
    assert!(err.is_err());
}

#[test]
fn emits_float_arithmetic() {
    // f64 の算術・比較は浮動小数命令で生成される。
    let ir = emit("fn f(a: f64, b: f64): bool {\n    return a + b == 4.0\n}");
    assert!(ir.contains("define i1 @f(double %arg0, double %arg1)"));
    assert!(ir.contains("fadd double"));
    assert!(ir.contains("fcmp oeq double"));
}

#[test]
fn emits_unsigned_division() {
    // 符号なし整数は udiv/icmp ult を選ぶ。
    let ir = emit("fn f(a: u32, b: u32): bool {\n    return a / b < b\n}");
    assert!(ir.contains("udiv i32"));
    assert!(ir.contains("icmp ult i32"));
}

#[test]
fn emits_wide_integer() {
    // i64 リテラル・演算は i64 幅で生成される。
    let ir = emit("fn f(): i64 {\n    let big: i64 = 5000000000\n    return big + 1\n}");
    assert!(ir.contains("define i64 @f()"));
    assert!(ir.contains("add i64"));
}

#[test]
fn runs_float_computation() {
    // 浮動小数の加算と比較が実行時に正しいこと。
    let src = "fn main(): i32 {\n    let x: f64 = 1.5\n    let y: f64 = 2.5\n    if x + y == 4.0 {\n        return 7\n    }\n    return 0\n}";
    if let Some(code) = run_exit_code(src, "float") {
        assert_eq!(code, 7);
    }
}

#[test]
fn runs_unsigned_division() {
    // 符号なし除算の実行結果。
    let src = "fn main(): i32 {\n    let a: u32 = 200\n    let b: u32 = 7\n    if a / b == 28 {\n        return 9\n    }\n    return 0\n}";
    if let Some(code) = run_exit_code(src, "udiv") {
        assert_eq!(code, 9);
    }
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
fn emits_reference_params_as_ptr() {
    // 参照は opaque ポインタ。値として使うときは暗黙にデリファレンス（load）する。
    let ir = emit("fn add(a: &i32, b: &i32): i32 {\n    return a + b\n}");
    assert!(ir.contains("define i32 @add(ptr %arg0, ptr %arg1)"));
    // 参照そのもの（ptr）を読み、続けて指す先の i32 を読む二段 load。
    assert!(ir.contains("load ptr, ptr"));
    assert!(ir.contains("load i32, ptr"));
    assert!(ir.contains("add i32"));
}

#[test]
fn runs_reference_deref_sum() {
    // `&x` で場所のアドレスを渡し、関数側で暗黙 deref して合計する。
    let src = "fn add(a: &i32, b: &i32): i32 {\n    return a + b\n}\nfn main(): i32 {\n    let x = 20\n    let y = 22\n    return add(&x, &y)\n}";
    if let Some(code) = run_exit_code(src, "ref_sum") {
        assert_eq!(code, 42);
    }
}

#[test]
fn runs_reference_local_binding() {
    // 参照をローカルに束縛し、値の文脈で使うと暗黙にデリファレンスされる。
    let src = "fn main(): i32 {\n    let x = 7\n    let r = &x\n    return r + 1\n}";
    if let Some(code) = run_exit_code(src, "ref_local") {
        assert_eq!(code, 8);
    }
}

#[test]
fn runs_mut_ref_and_bool_ref_condition() {
    // `&mut` の引き渡しと、`&bool` の条件での暗黙 deref。
    let src = "fn pick(flag: &bool, a: &i32, b: &i32): i32 {\n    if flag {\n        return a\n    }\n    return b\n}\nfn main(): i32 {\n    let cond = true\n    let mut x = 30\n    let y = 12\n    return pick(&cond, &mut x, &y)\n}";
    if let Some(code) = run_exit_code(src, "ref_mut_bool") {
        assert_eq!(code, 30);
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
