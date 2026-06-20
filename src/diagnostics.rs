//! miette を用いた診断。字句・構文エラーをソース付きで表示する。

use miette::{Diagnostic, LabeledSpan, NamedSource, Report, SourceSpan};
use thiserror::Error;

use crate::codegen::CodegenError;
use crate::lexer::LexError;
use crate::parser::ParseErr;
use crate::sema::{OwnershipError, ResolveError, TypeError};

#[derive(Error, Debug, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(iris::lex))]
pub struct LexDiagnostic {
    #[source_code]
    src: NamedSource<String>,
    message: String,
    #[label("ここ")]
    span: SourceSpan,
}

#[derive(Error, Debug, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(iris::parse))]
pub struct ParseDiagnostic {
    #[source_code]
    src: NamedSource<String>,
    message: String,
    #[label("ここ")]
    span: SourceSpan,
}

/// 名前解決エラー（複数件）。各エラーを 1 つのラベルとしてまとめて表示する。
#[derive(Error, Debug, Diagnostic)]
#[error("名前解決に失敗しました（{} 件）", labels.len())]
#[diagnostic(code(iris::resolve))]
pub struct ResolveDiagnostic {
    #[source_code]
    src: NamedSource<String>,
    #[label(collection)]
    labels: Vec<LabeledSpan>,
}

/// 型エラー（複数件）。各エラーを 1 つのラベルとしてまとめて表示する。
#[derive(Error, Debug, Diagnostic)]
#[error("型検査に失敗しました（{} 件）", labels.len())]
#[diagnostic(code(iris::typeck))]
pub struct TypeDiagnostic {
    #[source_code]
    src: NamedSource<String>,
    #[label(collection)]
    labels: Vec<LabeledSpan>,
}

/// 所有権（ムーブ）エラー（複数件）。
#[derive(Error, Debug, Diagnostic)]
#[error("所有権検査に失敗しました（{count} 件）")]
#[diagnostic(code(iris::ownership))]
pub struct OwnershipDiagnostic {
    #[source_code]
    src: NamedSource<String>,
    count: usize,
    #[label(collection)]
    labels: Vec<LabeledSpan>,
}

/// コード生成エラー（位置情報あり）。
#[derive(Error, Debug, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(iris::codegen))]
pub struct CodegenDiagnostic {
    #[source_code]
    src: NamedSource<String>,
    message: String,
    #[label("ここ")]
    span: SourceSpan,
}

/// 字句エラーを miette レポートへ変換する。
pub fn lex_report(name: &str, src: &str, err: LexError) -> Report {
    LexDiagnostic {
        src: NamedSource::new(name, src.to_string()),
        message: err.message,
        span: SourceSpan::new(err.offset.into(), 1),
    }
    .into()
}

/// 構文エラーを miette レポートへ変換する。
pub fn parse_report(name: &str, src: &str, err: ParseErr) -> Report {
    ParseDiagnostic {
        src: NamedSource::new(name, src.to_string()),
        message: err.message,
        span: err.span.into(),
    }
    .into()
}

/// 名前解決エラー（複数件）を 1 つの miette レポートへ変換する。
pub fn resolve_report(name: &str, src: &str, errors: Vec<ResolveError>) -> Report {
    let labels = errors
        .into_iter()
        .map(|e| LabeledSpan::new_with_span(Some(e.message), e.span))
        .collect();
    ResolveDiagnostic {
        src: NamedSource::new(name, src.to_string()),
        labels,
    }
    .into()
}

/// 型エラー（複数件）を 1 つの miette レポートへ変換する。
pub fn type_report(name: &str, src: &str, errors: Vec<TypeError>) -> Report {
    let labels = errors
        .into_iter()
        .map(|e| LabeledSpan::new_with_span(Some(e.message), e.span))
        .collect();
    TypeDiagnostic {
        src: NamedSource::new(name, src.to_string()),
        labels,
    }
    .into()
}

/// 所有権（ムーブ）エラー（複数件）を 1 つの miette レポートへ変換する。
pub fn ownership_report(name: &str, src: &str, errors: Vec<OwnershipError>) -> Report {
    let count = errors.len();
    let mut labels = Vec::new();
    for e in errors {
        labels.push(LabeledSpan::new_with_span(Some(e.message), e.span));
        if let Some((sec_span, sec_msg)) = e.secondary {
            labels.push(LabeledSpan::new_with_span(Some(sec_msg), sec_span));
        }
    }
    OwnershipDiagnostic {
        src: NamedSource::new(name, src.to_string()),
        count,
        labels,
    }
    .into()
}

/// コード生成エラーを miette レポートへ変換する。
pub fn codegen_report(name: &str, src: &str, err: CodegenError) -> Report {
    match err.span {
        Some(span) => CodegenDiagnostic {
            src: NamedSource::new(name, src.to_string()),
            message: err.message,
            span: span.into(),
        }
        .into(),
        None => miette::miette!("コード生成エラー: {}", err.message),
    }
}
