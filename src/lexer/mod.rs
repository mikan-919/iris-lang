pub mod tokenizer;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Bind,
    Next,
    Identifier(String),
    Integer(i64),
    String(String),
    Let,
    Fn,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Arrow,
    Whitespace,
    Newline,
    EOF,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Self { kind, line, column }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TokenKind::Bind => write!(f, "=::"),
            TokenKind::Next => write!(f, "::"),
            TokenKind::Identifier(s) => write!(f, "{}", s),
            TokenKind::Integer(n) => write!(f, "{}", n),
            TokenKind::String(s) => write!(f, "\"{}\"", s),
            TokenKind::Let => write!(f, "let"),
            TokenKind::Fn => write!(f, "fn"),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::Whitespace => write!(f, " "),
            TokenKind::Newline => write!(f, "\\n"),
            TokenKind::EOF => write!(f, "EOF"),
        }
    }
}
