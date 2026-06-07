use logos::Logos;
use std::fmt;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")] // whitespace
#[logos(skip r"//[^\n]*")]      // line comment
#[logos(skip r"/\*[^*]*\*+(?:[^/*][^*]*\*+)*/")]  // block comment
pub enum Token {
    // Keywords
    #[token("let")]
    Let,
    #[token("fn")]
    Fn,
    #[token("use")]
    Use,
    #[token("trait")]
    Trait,
    #[token("impl")]
    Impl,
    #[token("for")]
    For,
    #[token("match")]
    Match,

    // Backbone operators
    #[token("::")]
    Next,
    #[token(":~")]
    Await,
    #[token(":^")]
    Try,
    #[token(":!")]
    Force,
    #[token(":?")]
    Catch,
    #[token(":|")]
    Or,
    #[token(":>")]
    Tag,
    #[token(":&")]
    Join,
    #[token(":if")]
    ColonIf,
    #[token(":then")]
    ColonThen,
    #[token(":else")]
    ColonElse,
    #[token(":while")]
    ColonWhile,
    #[token(":await")]
    ColonAwait,
    #[token(":catch")]
    ColonCatch,

    // Symbols
    #[token("=")]
    Equal,
    #[token("->")]
    Arrow,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token(";")]
    Semi,
    #[token("$")]
    Dollar,
    #[token("|")]
    Pipe,
    #[token("!")]
    Bang,
    #[token("_", priority = 3)]
    Underscore,
    #[token("*")]
    Star,
    #[token("&&")]
    AndAnd,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,

    // Brackets
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,

    // Binary operators
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,

    // Literals
    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    IntLit(i64),
    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().parse::<f64>().ok())]
    FloatLit(f64),
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()
    })]
    StringLit(String),
    #[token("true")]
    True,
    #[token("false")]
    False,

    // Identifiers
    // Trait identifiers: #Foo
    #[regex(r"#[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice()[1..].to_string())]
    TraitIdent(String),
    // Normal identifiers
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Let => write!(f, "let"),
            Token::Fn => write!(f, "fn"),
            Token::Use => write!(f, "use"),
            Token::Trait => write!(f, "trait"),
            Token::Impl => write!(f, "impl"),
            Token::For => write!(f, "for"),
            Token::Match => write!(f, "match"),
            Token::Next => write!(f, "::"),
            Token::Await => write!(f, ":~"),
            Token::Try => write!(f, ":^"),
            Token::Force => write!(f, ":!"),
            Token::Catch => write!(f, ":?"),
            Token::Or => write!(f, ":|"),
            Token::Tag => write!(f, ":>"),
            Token::Join => write!(f, ":&"),
            Token::ColonIf => write!(f, ":if"),
            Token::ColonThen => write!(f, ":then"),
            Token::ColonElse => write!(f, ":else"),
            Token::ColonWhile => write!(f, ":while"),
            Token::ColonAwait => write!(f, ":await"),
            Token::ColonCatch => write!(f, ":catch"),
            Token::Equal => write!(f, "="),
            Token::Arrow => write!(f, "->"),
            Token::Colon => write!(f, ":"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::Semi => write!(f, ";"),
            Token::Dollar => write!(f, "$"),
            Token::Pipe => write!(f, "|"),
            Token::Bang => write!(f, "!"),
            Token::Underscore => write!(f, "_"),
            Token::Star => write!(f, "*"),
            Token::AndAnd => write!(f, "&&"),
            Token::Lt => write!(f, "<"),
            Token::Gt => write!(f, ">"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Slash => write!(f, "/"),
            Token::Percent => write!(f, "%"),
            Token::EqEq => write!(f, "=="),
            Token::NotEq => write!(f, "!="),
            Token::Le => write!(f, "<="),
            Token::Ge => write!(f, ">="),
            Token::IntLit(n) => write!(f, "{}", n),
            Token::FloatLit(n) => write!(f, "{}", n),
            Token::StringLit(s) => write!(f, "\"{}\"", s),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::TraitIdent(s) => write!(f, "#{}", s),
            Token::Ident(s) => write!(f, "{}", s),
        }
    }
}

/// Spanned token for use with lalrpop
pub type Spanned<Tok, Loc, Error> = Result<(Loc, Tok, Loc), Error>;

pub struct Lexer<'input> {
    inner: logos::SpannedIter<'input, Token>,
}

impl<'input> Lexer<'input> {
    pub fn new(input: &'input str) -> Self {
        Self {
            inner: Token::lexer(input).spanned(),
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Spanned<Token, usize, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(tok, span)| match tok {
            Ok(tok) => Ok((span.start, tok, span.end)),
            Err(_) => Err(LexError::InvalidToken {
                span: (span.start, span.end),
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LexError {
    #[error("invalid token at {span:?}")]
    InvalidToken { span: (usize, usize) },
}
