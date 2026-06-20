//! 所有権 DAG（グラフ構築）の速度計測。
//!
//!   cargo run --release --example bench_ownership
//!
//! 入力を作り分けて、所有権検査の中の各グラフ処理を分離して測る:
//! - 型グラフ（循環検出）: 型定義だけの入力 → check_ownership ≒ typegraph
//! - 値グラフ（provenance/借用競合）: 借用だらけの関数 → check_ownership ≒ flow+borrows

use std::time::Instant;

use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::sema::{check, check_ownership, resolve};

/// クロージャを `runs` 回実行し、最小所要時間（ミリ秒）を返す。
fn best_ms(runs: u32, mut f: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..runs {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    best
}

/// 所有権検査だけを測る（前段はあらかじめ済ませておく）。
fn measure_ownership(src: &str, runs: u32) -> (f64, usize) {
    let tokens = lex(src).expect("lex");
    let program = parse(&tokens).expect("parse");
    let resolution = resolve(&program).expect("resolve");
    let type_info = check(&program, &resolution).expect("typeck");
    let ms = best_ms(runs, || {
        check_ownership(&program, &resolution, &type_info).expect("ownership");
    });
    (ms, src.lines().count())
}

/// 型グラフ用: N 個の型定義が互いを所有して大きな DAG を作る（循環なし）。
fn gen_type_graph(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        if i + 3 < n {
            s.push_str(&format!(
                "type T{i} = struct {{\n    a: T{}\n    b: T{}\n    c: T{}\n    v: i32\n}}\n",
                i + 1,
                i + 2,
                i + 3
            ));
        } else {
            s.push_str(&format!("type T{i} = struct {{\n    v: i32\n}}\n"));
        }
    }
    s
}

/// 値グラフ用: M 個の場所を同時に借用する関数（同スコープで active が M まで伸びる）。
fn gen_borrow_heavy(m: usize) -> String {
    let mut s = String::from("type P = struct {\n    x: i32\n}\nfn stress() {\n");
    for i in 0..m {
        s.push_str(&format!("    let mut p{i} = P {{ x: {} }}\n", i % 100));
    }
    for i in 0..m {
        s.push_str(&format!("    let r{i} = &p{i}\n"));
    }
    s.push_str("}\n");
    s
}

/// provenance 用: 参照を次々に束ね直して借用グラフの鎖を作り、最後に返す。
fn gen_prov_chain(m: usize) -> String {
    // r0 = &x; r1 = r0; r2 = r1; ... rM = r(M-1); return rM
    let mut s = String::from("fn chain(x: &i32): &i32 {\n    let r0 = x\n");
    for i in 1..m {
        s.push_str(&format!("    let r{i} = r{}\n", i - 1));
    }
    s.push_str(&format!("    return r{}\n}}\n", m - 1));
    s
}

fn main() {
    println!("=== 型グラフ（循環検出 typegraph）===");
    println!("{:>8} {:>10} {:>14}", "型数", "行数", "所有権検査(ms)");
    for &n in &[200usize, 1000, 4000, 8000, 16000] {
        let (ms, lines) = measure_ownership(&gen_type_graph(n), 20);
        println!("{n:>8} {lines:>10} {ms:>14.3}");
    }

    println!("\n=== 値グラフ（借用競合 borrows + flow）===");
    println!("{:>8} {:>10} {:>14}", "借用数", "行数", "所有権検査(ms)");
    for &m in &[200usize, 500, 1000, 2000, 4000] {
        let (ms, lines) = measure_ownership(&gen_borrow_heavy(m), 20);
        println!("{m:>8} {lines:>10} {ms:>14.3}");
    }

    println!("\n=== provenance 鎖（flow の到達可能性）===");
    println!("{:>8} {:>10} {:>14}", "鎖長", "行数", "所有権検査(ms)");
    for &m in &[200usize, 500, 1000, 2000, 4000] {
        let (ms, lines) = measure_ownership(&gen_prov_chain(m), 20);
        println!("{m:>8} {lines:>10} {ms:>14.3}");
    }
}
