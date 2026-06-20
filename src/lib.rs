//! iris-lang コンパイラのフロントエンド。
//!
//! 現状は字句解析 → 構文解析までを提供する。所有権 DAG 解析・型推論・
//! コード生成は今後追加する。

pub mod ast;
pub mod codegen;
pub mod diagnostics;
pub mod lexer;
pub mod parser;
pub mod sema;
pub mod span;
pub mod token;

use ast::Program;
use sema::{Resolution, TypeInfo};

/// 最小の標準ライブラリ。各プログラムの先頭に前置する。
pub const STD: &str = include_str!("../std/std.iris");

/// 字句解析〜意味解析（名前解決・型検査・所有権検査）まで行う。
///
/// 標準ライブラリ ([`STD`]) をソースの先頭へ連結してから解析する。span が一意に
/// なるよう単一の文字列として扱う（診断にも連結後のソースを用いる）。
fn analyze(name: &str, src: &str) -> Result<(Program, Resolution, TypeInfo), miette::Report> {
    let combined = format!("{STD}\n{src}");
    let src = combined.as_str();
    let tokens = lexer::lex(src).map_err(|e| diagnostics::lex_report(name, src, e))?;
    let program = parser::parse(&tokens).map_err(|e| diagnostics::parse_report(name, src, e))?;
    let resolution =
        sema::resolve(&program).map_err(|errs| diagnostics::resolve_report(name, src, errs))?;
    let type_info = sema::check(&program, &resolution)
        .map_err(|errs| diagnostics::type_report(name, src, errs))?;
    sema::check_ownership(&program, &resolution, &type_info)
        .map_err(|errs| diagnostics::ownership_report(name, src, errs))?;
    Ok((program, resolution, type_info))
}

/// ソースを字句解析・構文解析し、名前解決・型検査・所有権検査まで行って AST を返す。
///
/// `name` は診断に表示するソース名（通常はファイルパス）。
pub fn compile(name: &str, src: &str) -> Result<Program, miette::Report> {
    let (program, _, _) = analyze(name, src)?;
    Ok(program)
}

/// ソースを最後まで解析し、LLVM IR（テキスト）を生成して返す。
pub fn compile_ir(name: &str, src: &str) -> Result<String, miette::Report> {
    let (program, resolution, type_info) = analyze(name, src)?;
    codegen::emit_module(&program, &resolution, &type_info)
        .map_err(|e| diagnostics::codegen_report(name, src, e))
}
