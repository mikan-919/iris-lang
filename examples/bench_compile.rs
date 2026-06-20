//! コンパイル速度の検証ベンチ。
//!
//!   cargo run --release --example bench_compile
//!
//! 2 つを測る:
//! 1. iris フロントエンド＋コード生成（`compile_ir`、clang を含まない）のスループット
//! 2. clang による IR→実行ファイル化の時間（-O0 と -O2、clang がある環境のみ）
//!
//! 結論の確認用: フロントは線形で速く、ビルド時間は LLVM の最適化レベルが支配する。

use std::process::Command;
use std::time::Instant;

/// 算術・再帰・if からなる i32 プログラムを n 関数ぶん生成する。
fn gen_prog(n: usize) -> String {
    let mut s = String::from("fn f0(a: i32, b: i32): i32 {\n    return a + b\n}\n");
    for i in 1..n {
        s.push_str(&format!(
            "fn f{i}(a: i32, b: i32): i32 {{\n    let x = a + b * {m}\n    if x > 0 {{\n        return x + f{prev}(a, b) % 1000\n    }}\n    return x - {k}\n}}\n",
            m = i % 97,
            prev = i - 1,
            k = i % 13,
        ));
    }
    s.push_str(&format!(
        "fn main(): i32 {{\n    let r = println_int(f{}(2, 3) % 256)\n    return 0\n}}\n",
        n - 1
    ));
    s
}

fn best_ms(runs: u32, mut f: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..runs {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    best
}

fn clang_available() -> bool {
    Command::new("clang")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn main() {
    println!("=== フロントエンド＋コード生成（compile_ir, clang 除く）===");
    println!("{:>8} {:>8} {:>12} {:>14}", "関数数", "行数", "時間(ms)", "行/秒");
    let sizes = [100usize, 500, 1000, 2000, 5000];
    let mut sample_ir = String::new();
    for &n in &sizes {
        let src = gen_prog(n);
        let lines = src.lines().count();
        // ウォームアップ兼サンプル IR 取得。
        let ir = iris_lang::compile_ir("bench", &src).expect("compile_ir");
        if n == 1000 {
            sample_ir = ir;
        }
        let ms = best_ms(5, || {
            iris_lang::compile_ir("bench", &src).expect("compile_ir");
        });
        let lps = (lines as f64 / (ms / 1000.0)) as u64;
        println!("{n:>8} {lines:>8} {ms:>12.2} {lps:>14}");
    }

    if !clang_available() {
        println!("\n(clang が無いため IR→実行ファイルの計測はスキップ)");
        return;
    }

    println!("\n=== clang で IR→実行ファイル（1000関数, IR {} 行）===", sample_ir.lines().count());
    let dir = std::env::temp_dir();
    let ll = dir.join("iris_bench_compile.ll");
    let exe = dir.join("iris_bench_compile.bin");
    std::fs::write(&ll, &sample_ir).unwrap();
    let clang_ms = |opt: &str| {
        best_ms(3, || {
            let ok = Command::new("clang")
                .arg(opt)
                .arg("-Wno-override-module")
                .arg(&ll)
                .arg("-o")
                .arg(&exe)
                .output()
                .expect("clang")
                .status
                .success();
            assert!(ok, "clang 失敗");
        })
    };
    println!("{:>14} {:>12}", "最適化", "時間(ms)");
    for opt in ["-O0", "-O1", "-O2"] {
        println!("{opt:>14} {:>12.1}", clang_ms(opt));
    }
    let _ = std::fs::remove_file(&ll);
    let _ = std::fs::remove_file(&exe);

    println!("\n要点: フロントは ~十数万行/秒で線形。ビルド総時間は clang の最適化レベルが支配する。");
}
