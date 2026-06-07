use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use crate::lexer::LexError;

/// Irisのパースエラー
#[derive(Debug, Error, Diagnostic)]
#[error("parse error")]
pub struct ParseError {
    #[source_code]
    pub src: miette::NamedSource<String>,
    #[related]
    pub errors: Vec<ParseErrorKind>,
}

#[derive(Debug, Error, Diagnostic)]
pub enum ParseErrorKind {
    #[error("unexpected token: {token}")]
    #[diagnostic(code(iris::parse::unexpected_token))]
    UnexpectedToken {
        token: String,
        #[label("here")]
        span: SourceSpan,
    },

    #[error("unexpected end of file")]
    #[diagnostic(code(iris::parse::unexpected_eof))]
    UnexpectedEof {
        #[label("expected more tokens")]
        span: SourceSpan,
    },

    #[error("lexer error")]
    #[diagnostic(code(iris::lex::invalid_token))]
    LexError {
        #[label("invalid token")]
        span: SourceSpan,
    },

    #[error("{message}")]
    #[diagnostic(code(iris::parse::error))]
    Other {
        message: String,
        #[label("{message}")]
        span: SourceSpan,
    },
}

/// lalrpop の ParseError を miette 向けに変換
pub fn convert_parse_error(
    src: miette::NamedSource<String>,
    err: lalrpop_util::ParseError<usize, crate::lexer::Token, LexError>,
) -> ParseError {
    use lalrpop_util::ParseError as LP;

    let kind = match err {
        LP::InvalidToken { location } => ParseErrorKind::LexError {
            span: (location, 1).into(),
        },
        LP::UnrecognizedEof { location, .. } => ParseErrorKind::UnexpectedEof {
            span: (location, 1).into(),
        },
        LP::UnrecognizedToken { token: (l, tok, r), .. } => ParseErrorKind::UnexpectedToken {
            token: tok.to_string(),
            span: (l, r - l).into(),
        },
        LP::ExtraToken { token: (l, tok, r) } => ParseErrorKind::UnexpectedToken {
            token: tok.to_string(),
            span: (l, r - l).into(),
        },
        LP::User { error: LexError::InvalidToken { span } } => ParseErrorKind::LexError {
            span: (span.0, span.1 - span.0).into(),
        },
    };

    ParseError {
        src,
        errors: vec![kind],
    }
}

/// Irisの型エラー
#[derive(Debug, Error, Diagnostic)]
#[error("type error")]
pub struct TypeDiagnostic {
    #[source_code]
    pub src: miette::NamedSource<String>,
    #[related]
    pub errors: Vec<TypeDiagnosticErrorKind>,
}

#[derive(Debug, Error, Diagnostic)]
pub enum TypeDiagnosticErrorKind {
    #[error("{message}")]
    #[diagnostic(code(iris::typecheck::error))]
    TypeError {
        message: String,
        #[label("{message}")]
        span: miette::SourceSpan,
    },
}

/// TypeChecker の TypeError を miette 向けに変換
pub fn convert_type_error(
    src: miette::NamedSource<String>,
    err: crate::typecheck::TypeError,
) -> TypeDiagnostic {
    let span = err.span();
    let message = err.to_string_message();
    
    // スパン情報 (start, end) から (offset, length) を算出
    let offset = span.0;
    let length = if span.1 >= span.0 { span.1 - span.0 } else { 0 };

    TypeDiagnostic {
        src,
        errors: vec![TypeDiagnosticErrorKind::TypeError {
            message,
            span: (offset, length).into(),
        }],
    }
}
