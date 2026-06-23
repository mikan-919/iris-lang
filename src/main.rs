//! iris-lang CLI。
//!
//! - `iris <file.iris>`               … AST を表示
//! - `iris --emit-llvm <file.iris>`   … LLVM IR（テキスト）を表示
//! - `iris build [--release] [-o OUT] <file.iris>` … 実行ファイルを生成
//! - `iris run   [--release] <file.iris>`          … 生成して即実行
//!
//! ビルドは既定で **-O0**（速い開発ビルド）。`--release` で **-O2**。
//! 実行ファイル化・リンクは `clang` に委ねる（テキスト IR を渡す）。
//!
//! エラーは miette で整形して表示する。

use std::path::Path;
use std::process::{Command, ExitCode};

struct Args {
    subcmd: Option<String>,
    emit_llvm: bool,
    release: bool,
    out: Option<String>,
    path: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args {
        subcmd: None,
        emit_llvm: false,
        release: false,
        out: None,
        path: None,
    };
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut it = raw.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--emit-llvm" => a.emit_llvm = true,
            "--release" => a.release = true,
            "-o" => a.out = it.next().cloned(),
            "build" | "run" if a.subcmd.is_none() && a.path.is_none() => {
                a.subcmd = Some(arg.clone());
            }
            _ => a.path = Some(arg.clone()),
        }
    }
    a
}

fn main() -> ExitCode {
    let args = parse_args();
    let Some(path) = args.path.clone() else {
        eprintln!("使い方:");
        eprintln!("  iris <file.iris>                          # AST を表示");
        eprintln!("  iris --emit-llvm <file.iris>             # LLVM IR を表示");
        eprintln!("  iris build [--release] [-o OUT] <file>   # 実行ファイルを生成");
        eprintln!("  iris run   [--release] <file>            # 生成して即実行");
        return ExitCode::FAILURE;
    };

    let src = match std::fs::read_to_string(&path) {
        Ok(src) => src,
        Err(err) => {
            eprintln!("`{path}` を読み込めませんでした: {err}");
            return ExitCode::FAILURE;
        }
    };

    match args.subcmd.as_deref() {
        Some("build") => build(&path, &src, args.release, args.out.as_deref(), false),
        Some("run") => build(&path, &src, args.release, args.out.as_deref(), true),
        _ if args.emit_llvm => emit_ir(&path, &src),
        _ => dump_ast(&path, &src),
    }
}

fn dump_ast(path: &str, src: &str) -> ExitCode {
    match iris_lang::compile(path, src) {
        Ok(program) => {
            println!("{program:#?}");
            ExitCode::SUCCESS
        }
        Err(report) => {
            eprintln!("{report:?}");
            ExitCode::FAILURE
        }
    }
}

fn emit_ir(path: &str, src: &str) -> ExitCode {
    match iris_lang::compile_ir(path, src) {
        Ok(ir) => {
            print!("{ir}");
            ExitCode::SUCCESS
        }
        Err(report) => {
            eprintln!("{report:?}");
            ExitCode::FAILURE
        }
    }
}

/// LLVM IR を生成し、clang で実行ファイルにする。`run` が真ならそのまま実行する。
fn build(path: &str, src: &str, release: bool, out: Option<&str>, run: bool) -> ExitCode {
    // 1. フロントエンド〜コード生成。
    let ir = match iris_lang::compile_ir(path, src) {
        Ok(ir) => ir,
        Err(report) => {
            eprintln!("{report:?}");
            return ExitCode::FAILURE;
        }
    };

    // 2. 出力先と一時 IR ファイル。
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a");
    let out_path = out
        .map(str::to_string)
        .unwrap_or_else(|| stem.to_string());
    let pid = std::process::id();
    let ll = std::env::temp_dir().join(format!("iris-{stem}-{pid}.ll"));
    if let Err(e) = std::fs::write(&ll, &ir) {
        eprintln!("一時ファイルを書けませんでした: {e}");
        return ExitCode::FAILURE;
    }

    // run の場合は一時ファイルへ出力する。
    let exe = if run {
        std::env::temp_dir()
            .join(format!("iris-{stem}-{pid}.bin"))
            .to_string_lossy()
            .into_owned()
    } else {
        out_path.clone()
    };

    // 3. clang で実行ファイル化。既定 -O0、--release で -O2。
    // `-nostartfiles`: crt0.o 等の CRT を除外（`@_start` は iris が自前で生成）。
    // libc 自体は動的リンクのまま（puts/strlen 等はまだ libc 依存）。
    let opt = if release { "-O2" } else { "-O0" };
    let clang = Command::new("clang")
        .arg(opt)
        .arg("-nostartfiles")
        .arg("-Wno-override-module") // モジュール triple 上書きの警告を抑制
        .arg(&ll)
        .arg("-o")
        .arg(&exe)
        .output();
    let _ = std::fs::remove_file(&ll);

    let clang = match clang {
        Ok(o) => o,
        Err(e) => {
            eprintln!("clang を起動できませんでした（インストールされていますか？）: {e}");
            return ExitCode::FAILURE;
        }
    };
    if !clang.status.success() {
        eprintln!("clang がリンク/コンパイルに失敗しました:");
        eprintln!("{}", String::from_utf8_lossy(&clang.stderr));
        return ExitCode::FAILURE;
    }

    if !run {
        let profile = if release { "release, -O2" } else { "debug, -O0" };
        eprintln!("ビルド完了: {exe} ({profile})");
        return ExitCode::SUCCESS;
    }

    // 4. run: 生成した実行ファイルを起動し、終了コードを引き継ぐ。
    let status = Command::new(&exe).status();
    let _ = std::fs::remove_file(&exe);
    match status {
        Ok(s) => match s.code() {
            Some(code) => ExitCode::from(code as u8),
            None => ExitCode::FAILURE,
        },
        Err(e) => {
            eprintln!("実行に失敗しました: {e}");
            ExitCode::FAILURE
        }
    }
}
