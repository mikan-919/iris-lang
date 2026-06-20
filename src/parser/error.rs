//! 構文解析エラー。span とメッセージを保持し、後段で miette 診断に変換する。

use nom::error::{ErrorKind, ParseError};

use crate::span::Span;
use crate::token::Token;

use super::tokens::Tokens;

#[derive(Debug, Clone)]
pub struct ParseErr {
    pub span: Span,
    pub message: String,
}

impl ParseErr {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        ParseErr {
            span,
            message: message.into(),
        }
    }

    /// 「<期待> が必要ですが、<実際> が見つかりました」形式のエラーを作る。
    pub fn expected(what: impl Into<String>, found: &Token) -> Self {
        ParseErr {
            span: found.span,
            message: format!(
                "{} が必要ですが、{} が見つかりました",
                what.into(),
                found.kind.describe()
            ),
        }
    }
}

impl<'a> ParseError<Tokens<'a>> for ParseErr {
    fn from_error_kind(input: Tokens<'a>, kind: ErrorKind) -> Self {
        ParseErr {
            span: input.span(),
            message: format!("構文エラー ({kind:?})"),
        }
    }

    fn append(_input: Tokens<'a>, _kind: ErrorKind, other: Self) -> Self {
        other
    }
}
