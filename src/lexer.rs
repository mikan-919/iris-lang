//! 字句解析器。nom + nom_locate でソースを span 付きトークン列へ変換する。
//!
//! 改行 (`\n`) は文の区切りとして意味を持つため `Newline` トークンとして残す。
//! 空白・タブ・行コメント (`//`) は読み飛ばす。

use nom::Parser;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_till, take_while};
use nom::character::complete::{char, digit1, satisfy};
use nom::combinator::{opt, recognize};
use nom::sequence::{pair, preceded};
use nom_locate::LocatedSpan;

use crate::span::Span;
use crate::token::{Token, TokenKind};

type LSpan<'a> = LocatedSpan<&'a str>;

/// 字句解析の失敗。未知の文字に遭遇した位置を保持する。
#[derive(Debug, Clone)]
pub struct LexError {
    pub offset: usize,
    pub message: String,
}

/// ソース文字列をトークン列に変換する。末尾には必ず `Eof` を付与する。
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    lex_at(src, 0)
}

/// `base_offset` を先頭オフセットとしてトークン列に変換する。
///
/// モジュールソースを combined ソースと異なるオフセット空間で解析することで、
/// span の衝突（`types: HashMap<Span, Ty>` のコリジョン）を防ぐ。
pub fn lex_at(src: &str, base_offset: usize) -> Result<Vec<Token>, LexError> {
    // SAFETY: base_offset は byte 単位の正しいオフセット。fragment は有効な UTF-8。
    let mut input = unsafe { LSpan::new_from_raw_offset(base_offset, 1, src, ()) };
    let mut tokens = Vec::new();

    loop {
        input = skip_trivia(input);
        if input.fragment().is_empty() {
            break;
        }

        match next_token(input) {
            Ok((rest, token)) => {
                input = rest;
                tokens.push(token);
            }
            Err(_) => {
                let offset = input.location_offset();
                let ch = input.fragment().chars().next().unwrap_or('\0');
                return Err(LexError {
                    offset,
                    message: format!("予期しない文字 `{ch}`"),
                });
            }
        }
    }

    let eof_span = Span::new(base_offset + src.len(), 0);
    tokens.push(Token::new(TokenKind::Eof, eof_span));
    Ok(tokens)
}

/// 空白・タブ・キャリッジリターン・行コメントを読み飛ばす。改行は残す。
fn skip_trivia(mut input: LSpan) -> LSpan {
    loop {
        // 空白・タブ・CR
        if let Ok((rest, matched)) =
            take_while::<_, _, ()>(|c| c == ' ' || c == '\t' || c == '\r')(input)
            && !matched.fragment().is_empty()
        {
            input = rest;
            continue;
        }
        // 行コメント `// ...`
        if input.fragment().starts_with("//")
            && let Ok((rest, _)) =
                recognize(pair(tag::<_, _, ()>("//"), take_till(|c| c == '\n'))).parse(input)
        {
            input = rest;
            continue;
        }
        break;
    }
    input
}

/// 先頭の 1 トークンを取り出す。
fn next_token(input: LSpan) -> nom::IResult<LSpan, Token> {
    alt((
        lex_newline,
        lex_number,
        lex_string,
        lex_ident_or_keyword,
        lex_symbol,
    ))
    .parse(input)
}

/// 開始位置と終了位置から span を組み立てる。
fn mk_span(start: &LSpan, end: &LSpan) -> Span {
    let s = start.location_offset();
    Span::new(s, end.location_offset() - s)
}

fn lex_newline(input: LSpan) -> nom::IResult<LSpan, Token> {
    let start = input;
    let (input, _) = char('\n')(input)?;
    Ok((input, Token::new(TokenKind::Newline, mk_span(&start, &input))))
}

fn lex_number(input: LSpan) -> nom::IResult<LSpan, Token> {
    let start = input;
    let (input, int_part) = digit1(input)?;
    let (input, frac_part) = opt(preceded(char('.'), digit1)).parse(input)?;

    let kind = match frac_part {
        Some(frac) => {
            let text = format!("{}.{}", int_part.fragment(), frac.fragment());
            let value: f64 = text.parse().unwrap_or(0.0);
            TokenKind::Float(value)
        }
        None => {
            let value: i64 = int_part.fragment().parse().unwrap_or(0);
            TokenKind::Int(value)
        }
    };

    Ok((input, Token::new(kind, mk_span(&start, &input))))
}

fn lex_string(input: LSpan) -> nom::IResult<LSpan, Token> {
    let start = input;
    let (mut input, _) = char('"')(input)?;
    let mut value = String::new();

    loop {
        let frag = input.fragment();
        let mut chars = frag.chars();
        match chars.next() {
            None => {
                // 閉じられていない文字列。ここでは nom のエラーにして上位で報告する。
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Char,
                )));
            }
            Some('"') => {
                let (rest, _) = char('"')(input)?;
                input = rest;
                break;
            }
            Some('\\') => {
                let (rest, _) = char('\\')(input)?;
                let (rest, esc) = nom::character::complete::anychar(rest)?;
                let decoded = match esc {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    '0' => '\0',
                    other => other,
                };
                value.push(decoded);
                input = rest;
            }
            Some(c) => {
                let (rest, _) = nom::character::complete::anychar(input)?;
                value.push(c);
                input = rest;
            }
        }
    }

    Ok((
        input,
        Token::new(TokenKind::Str(value), mk_span(&start, &input)),
    ))
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn lex_ident_or_keyword(input: LSpan) -> nom::IResult<LSpan, Token> {
    let start = input;
    let (input, matched) =
        recognize(pair(satisfy(is_ident_start), take_while(is_ident_continue))).parse(input)?;

    let text = *matched.fragment();
    let kind = match text {
        "fn" => TokenKind::Fn,
        "let" => TokenKind::Let,
        "const" => TokenKind::Const,
        "return" => TokenKind::Return,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "while" => TokenKind::While,
        "loop" => TokenKind::Loop,
        "for" => TokenKind::For,
        "in" => TokenKind::In,
        "break" => TokenKind::Break,
        "continue" => TokenKind::Continue,
        "pub" => TokenKind::Pub,
        "mut" => TokenKind::Mut,
        "type" => TokenKind::Type,
        "struct" => TokenKind::Struct,
        "enum" => TokenKind::Enum,
        "extern" => TokenKind::Extern,
        "match" => TokenKind::Match,
        "use" => TokenKind::Use,
        "as" => TokenKind::As,
        "true" => TokenKind::Bool(true),
        "false" => TokenKind::Bool(false),
        _ => TokenKind::Ident(text.to_string()),
    };

    Ok((input, Token::new(kind, mk_span(&start, &input))))
}

fn lex_symbol(input: LSpan) -> nom::IResult<LSpan, Token> {
    let start = input;

    // 3 文字の記号を最優先で試す（最長一致）。`..=` は `..` より先に判定する。
    let three_char: &[(&str, TokenKind)] = &[("..=", TokenKind::DotDotEq)];
    for (sym, kind) in three_char {
        if let Ok((rest, _)) = tag::<_, _, ()>(*sym)(input) {
            return Ok((rest, Token::new(kind.clone(), mk_span(&start, &rest))));
        }
    }

    // 2 文字以上の記号を先に試す。`..` は `.` より先に判定する。
    let two_char: &[(&str, TokenKind)] = &[
        ("->", TokenKind::Arrow),
        ("==", TokenKind::EqEq),
        ("!=", TokenKind::NotEq),
        ("<=", TokenKind::LtEq),
        (">=", TokenKind::GtEq),
        ("&&", TokenKind::AndAnd),
        ("||", TokenKind::OrOr),
        ("..", TokenKind::DotDot),
    ];
    for (sym, kind) in two_char {
        if let Ok((rest, _)) = tag::<_, _, ()>(*sym)(input) {
            return Ok((rest, Token::new(kind.clone(), mk_span(&start, &rest))));
        }
    }

    let one_char: &[(char, TokenKind)] = &[
        ('(', TokenKind::LParen),
        (')', TokenKind::RParen),
        ('{', TokenKind::LBrace),
        ('}', TokenKind::RBrace),
        ('[', TokenKind::LBracket),
        (']', TokenKind::RBracket),
        (',', TokenKind::Comma),
        (':', TokenKind::Colon),
        ('.', TokenKind::Dot),
        ('+', TokenKind::Plus),
        ('-', TokenKind::Minus),
        ('*', TokenKind::Star),
        ('/', TokenKind::Slash),
        ('%', TokenKind::Percent),
        ('=', TokenKind::Assign),
        ('<', TokenKind::Lt),
        ('>', TokenKind::Gt),
        ('!', TokenKind::Bang),
        ('?', TokenKind::Question),
        ('&', TokenKind::Amp),
        ('|', TokenKind::Pipe),
        ('#', TokenKind::Hash),
    ];
    for (sym, kind) in one_char {
        if let Ok((rest, _)) = char::<_, ()>(*sym)(input) {
            return Ok((rest, Token::new(kind.clone(), mk_span(&start, &rest))));
        }
    }

    Err(nom::Err::Error(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Char,
    )))
}
