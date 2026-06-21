//! 字句解析が生成するトークン。各トークンは種別と span を持つ。

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // リテラル
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Ident(String),

    // キーワード
    Fn,
    Let,
    Const,
    Return,
    If,
    Else,
    While,
    Loop,
    Break,
    Continue,
    Pub,
    Mut,
    Type,
    Struct,
    Enum,
    Extern,
    Match,

    // 区切り・括弧
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    LBracket, // [
    RBracket, // ]
    Comma,    // ,
    Colon,    // :
    Dot,      // .
    DotDot,   // ..  (範囲パターン・排他上限)
    DotDotEq, // ..= (範囲パターン・包含上限)
    Arrow,    // ->

    // 演算子
    Plus,     // +
    Minus,    // -
    Star,     // *
    Slash,    // /
    Percent,  // %
    Assign,   // =
    EqEq,     // ==
    NotEq,    // !=
    Lt,       // <
    LtEq,     // <=
    Gt,       // >
    GtEq,     // >=
    AndAnd,   // &&
    OrOr,     // ||
    Bang,     // !  (エラー伝播の後置演算子)
    Question, // ?  (三項演算子)
    Amp,      // &  (参照)
    Hash,     // #  (メソッド名衝突の修飾子。ADR-0004)

    // 構造
    Newline,
    Eof,
}

impl TokenKind {
    /// 診断メッセージ向けの人間可読な名前。
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Int(_) => "整数リテラル".into(),
            TokenKind::Float(_) => "浮動小数点リテラル".into(),
            TokenKind::Str(_) => "文字列リテラル".into(),
            TokenKind::Bool(_) => "真偽値リテラル".into(),
            TokenKind::Ident(_) => "識別子".into(),
            TokenKind::Fn => "`fn`".into(),
            TokenKind::Let => "`let`".into(),
            TokenKind::Const => "`const`".into(),
            TokenKind::Return => "`return`".into(),
            TokenKind::If => "`if`".into(),
            TokenKind::Else => "`else`".into(),
            TokenKind::While => "`while`".into(),
            TokenKind::Loop => "`loop`".into(),
            TokenKind::Break => "`break`".into(),
            TokenKind::Continue => "`continue`".into(),
            TokenKind::Pub => "`pub`".into(),
            TokenKind::Mut => "`mut`".into(),
            TokenKind::Type => "`type`".into(),
            TokenKind::Struct => "`struct`".into(),
            TokenKind::Enum => "`enum`".into(),
            TokenKind::Extern => "`extern`".into(),
            TokenKind::Match => "`match`".into(),
            TokenKind::LParen => "`(`".into(),
            TokenKind::RParen => "`)`".into(),
            TokenKind::LBrace => "`{`".into(),
            TokenKind::RBrace => "`}`".into(),
            TokenKind::LBracket => "`[`".into(),
            TokenKind::RBracket => "`]`".into(),
            TokenKind::Comma => "`,`".into(),
            TokenKind::Colon => "`:`".into(),
            TokenKind::Dot => "`.`".into(),
            TokenKind::DotDot => "`..`".into(),
            TokenKind::DotDotEq => "`..=`".into(),
            TokenKind::Arrow => "`->`".into(),
            TokenKind::Plus => "`+`".into(),
            TokenKind::Minus => "`-`".into(),
            TokenKind::Star => "`*`".into(),
            TokenKind::Slash => "`/`".into(),
            TokenKind::Percent => "`%`".into(),
            TokenKind::Assign => "`=`".into(),
            TokenKind::EqEq => "`==`".into(),
            TokenKind::NotEq => "`!=`".into(),
            TokenKind::Lt => "`<`".into(),
            TokenKind::LtEq => "`<=`".into(),
            TokenKind::Gt => "`>`".into(),
            TokenKind::GtEq => "`>=`".into(),
            TokenKind::AndAnd => "`&&`".into(),
            TokenKind::OrOr => "`||`".into(),
            TokenKind::Bang => "`!`".into(),
            TokenKind::Question => "`?`".into(),
            TokenKind::Amp => "`&`".into(),
            TokenKind::Hash => "`#`".into(),
            TokenKind::Newline => "改行".into(),
            TokenKind::Eof => "入力の終端".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Token { kind, span }
    }
}
