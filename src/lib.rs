//! iris-lang コンパイラのフロントエンド。
//!
//! 現状は字句解析 → 構文解析までを提供する。所有権 DAG 解析・型推論・
//! コード生成は今後追加する。

pub mod ast;
pub mod codegen;
pub mod diagnostics;
pub mod lexer;
pub mod module;
pub mod parser;
pub mod sema;
pub mod span;
pub mod token;

use std::collections::HashMap;
use std::path::Path;

use ast::{Item, Program, UseDecl};
use sema::{Resolution, TypeInfo};

/// 最小の標準ライブラリ（プレリュード）。各プログラムの先頭に自動前置する。
pub const PRELUDE: &str = include_str!("../std/prelude.iris");

/// 字句解析〜意味解析（名前解決・型検査・所有権検査）まで行う。
///
/// 1. プレリュード（`std/prelude.iris`）をソース先頭に前置し、単一ソースとして解析する。
/// 2. パース後、プログラム中の `use` 宣言を抽出してモジュールローダーで処理する。
///    - `use std.prelude.*` は自動前置済みのため無視する。
///    - それ以外の `use` はファイルを読み込み、pub アイテムをプログラム先頭に prepend する。
/// 3. `use` 宣言の処理結果を名前解決に渡し、モジュールパス呼び出しも解決する。
///
/// `base_dir`: ユーザーモジュール解決の基準ディレクトリ（通常はコンパイル対象ファイルの親）。
fn analyze(
    name: &str,
    src: &str,
    base_dir: Option<&Path>,
) -> Result<(Program, Resolution, TypeInfo), miette::Report> {
    // プレリュードを先頭に前置して一つのソースにする（span の連続性を保つ）。
    let combined = format!("{PRELUDE}\n{src}");
    let combined_src = combined.as_str();

    let tokens =
        lexer::lex(combined_src).map_err(|e| diagnostics::lex_report(name, combined_src, e))?;
    let mut program = parser::parse(&tokens)
        .map_err(|e| diagnostics::parse_report(name, combined_src, e))?;

    // `use` 宣言を抽出し、モジュールローダーで処理する。
    let use_decls: Vec<UseDecl> = program
        .items
        .iter()
        .filter_map(|i| {
            if let Item::Use(u) = i {
                // `use std.prelude.*` はプレリュードが自動前置済みなので無視する。
                if u.path == ["std", "prelude"] {
                    return None;
                }
                Some(u.clone())
            } else {
                None
            }
        })
        .collect();

    let (prepend_items, module_namespaces, mod_errors) = if use_decls.is_empty() {
        (Vec::new(), HashMap::new(), Vec::new())
    } else {
        let mut loader = module::ModuleLoader::new(base_dir, combined.len());
        loader.process_use_decls(&use_decls)
    };

    // モジュールロードエラーがあれば最初の 1 件を返す（複数エラーは将来対応）。
    if let Some(e) = mod_errors.first() {
        return Err(miette::miette!("{}", e.message));
    }

    // モジュールから得た pub アイテムをプログラム先頭に挿入する。
    if !prepend_items.is_empty() {
        let mut new_items = prepend_items;
        new_items.extend(program.items);
        program.items = new_items;
    }

    // 名前解決（モジュール名前空間を渡す）。
    let resolution = sema::resolve(&program, module_namespaces)
        .map_err(|errs| diagnostics::resolve_report(name, combined_src, errs))?;

    // 型検査・所有権検査。
    let type_info = sema::check(&program, &resolution)
        .map_err(|errs| diagnostics::type_report(name, combined_src, errs))?;
    sema::check_ownership(&program, &resolution, &type_info)
        .map_err(|errs| diagnostics::ownership_report(name, combined_src, errs))?;

    Ok((program, resolution, type_info))
}

/// ソースを字句解析・構文解析し、名前解決・型検査・所有権検査まで行って AST を返す。
///
/// `name` は診断に表示するソース名（通常はファイルパス）。
pub fn compile(name: &str, src: &str) -> Result<Program, miette::Report> {
    let base_dir = Path::new(name).parent();
    let (program, _, _) = analyze(name, src, base_dir)?;
    Ok(program)
}

/// ソースを最後まで解析し、LLVM IR（テキスト）を生成して返す。
///
/// `name` はファイルパス（モジュール解決の基準ディレクトリを導出する）。
pub fn compile_ir(name: &str, src: &str) -> Result<String, miette::Report> {
    let base_dir = Path::new(name).parent();
    let (program, resolution, type_info) = analyze(name, src, base_dir)?;
    codegen::emit_module(&program, &resolution, &type_info)
        .map_err(|e| diagnostics::codegen_report(name, src, e))
}

/// LSP 向けの診断エントリ。オフセットはユーザーソース先頭からのバイト数（プレリュード除く）。
pub struct LspDiagnostic {
    pub offset: usize,
    pub len: usize,
    pub message: String,
}

/// ソースを全フェーズで解析し、LSP 向けの診断リストを返す。
///
/// - エラーがなければ空ベクタ。
/// - プレリュード内のエラーは除外する（std のバグはユーザーに見せない）。
/// - `name` は診断に表示するファイル名（モジュール解決の基準ディレクトリにも使う）。
pub fn check_diagnostics(name: &str, src: &str) -> Vec<LspDiagnostic> {
    let prelude_len = PRELUDE.len() + 1; // PRELUDE + "\n"
    let combined = format!("{PRELUDE}\n{src}");
    let combined_src = combined.as_str();

    let mut diags: Vec<LspDiagnostic> = Vec::new();

    // --- 字句解析 ---
    let tokens = match lexer::lex(combined_src) {
        Ok(t) => t,
        Err(e) => {
            if e.offset >= prelude_len {
                diags.push(LspDiagnostic {
                    offset: e.offset - prelude_len,
                    len: 1,
                    message: e.message,
                });
            }
            return diags;
        }
    };

    // --- 構文解析 ---
    let mut program = match parser::parse(&tokens) {
        Ok(p) => p,
        Err(e) => {
            if e.span.offset >= prelude_len {
                diags.push(LspDiagnostic {
                    offset: e.span.offset - prelude_len,
                    len: e.span.len,
                    message: e.message,
                });
            }
            return diags;
        }
    };

    // --- モジュールロード ---
    let base_dir = Path::new(name).parent();
    let use_decls: Vec<UseDecl> = program
        .items
        .iter()
        .filter_map(|i| {
            if let Item::Use(u) = i {
                if u.path == ["std", "prelude"] { return None; }
                Some(u.clone())
            } else {
                None
            }
        })
        .collect();

    let (prepend_items, module_namespaces, _mod_errors) = if use_decls.is_empty() {
        (Vec::new(), HashMap::new(), Vec::new())
    } else {
        let mut loader = module::ModuleLoader::new(base_dir, combined.len());
        loader.process_use_decls(&use_decls)
    };
    // mod_errors は LSP では無視（ファイル未保存 / 解決失敗は silent）

    if !prepend_items.is_empty() {
        let mut new_items = prepend_items;
        new_items.extend(program.items);
        program.items = new_items;
    }

    // --- 名前解決 ---
    let resolution = match sema::resolve(&program, module_namespaces) {
        Ok(r) => r,
        Err(errors) => {
            for e in errors {
                if e.span.offset >= prelude_len {
                    diags.push(LspDiagnostic {
                        offset: e.span.offset - prelude_len,
                        len: e.span.len,
                        message: e.message,
                    });
                }
            }
            return diags;
        }
    };

    // --- 型検査 ---
    let type_info = match sema::check(&program, &resolution) {
        Ok(ti) => ti,
        Err(errors) => {
            for e in errors {
                if e.span.offset >= prelude_len {
                    diags.push(LspDiagnostic {
                        offset: e.span.offset - prelude_len,
                        len: e.span.len,
                        message: e.message,
                    });
                }
            }
            return diags;
        }
    };

    // --- 所有権検査 ---
    if let Err(errors) = sema::check_ownership(&program, &resolution, &type_info) {
        for e in errors {
            if e.span.offset >= prelude_len {
                diags.push(LspDiagnostic {
                    offset: e.span.offset - prelude_len,
                    len: e.span.len,
                    message: e.message,
                });
            }
            // secondary span はメモ程度なので LSP では省略
        }
    }

    diags
}
