pub mod tokenizer;

#[cfg(test)]
mod tokenizer_tests;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Bind,
    Next,
    Await,
    Try,
    Force,
    Catch,
    Or,
    Tag,
    Join,
    Colon,
    Plus,
    Minus,
    Star,
    Slash,
    LineComment(String),
    DocComment(String),
    BlockComment(String),
    Identifier(String),
    Integer(i64),
    String(String),
    Let,
    Fn,
    Export,
    Match,
    For,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Arrow,
    Pipe,
    At,
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
            TokenKind::Await => write!(f, ":~"),
            TokenKind::Try => write!(f, ":^"),
            TokenKind::Force => write!(f, ":!"),
            TokenKind::Catch => write!(f, ":?"),
            TokenKind::Or => write!(f, ":|"),
            TokenKind::Tag => write!(f, ":>"),
            TokenKind::Join => write!(f, ":&"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::LineComment(s) => write!(f, "// {}", s),
            TokenKind::DocComment(s) => write!(f, "/// {}", s),
            TokenKind::BlockComment(s) => write!(f, "/* {} */", s),
            TokenKind::Identifier(s) => write!(f, "{}", s),
            TokenKind::Integer(n) => write!(f, "{}", n),
            TokenKind::String(s) => write!(f, "\"{}\"", s),
            TokenKind::Let => write!(f, "let"),
            TokenKind::Fn => write!(f, "fn"),
            TokenKind::Export => write!(f, "export"),
            TokenKind::Match => write!(f, "match"),
            TokenKind::For => write!(f, "for"),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::At => write!(f, "@"),
            TokenKind::Whitespace => write!(f, " "),
            TokenKind::Newline => write!(f, "\\n"),
            TokenKind::EOF => write!(f, "EOF"),
        }
    }
}
