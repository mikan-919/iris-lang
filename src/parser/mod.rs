//! 構文解析器。字句解析が生成したトークン列から AST を構築する。
//!
//! 式は優先順位ごとに段を分けて解析する（precedence climbing）。
//! 文は改行で区切られる。

mod error;
mod tokens;

pub use error::ParseErr;
use tokens::Tokens;

use nom::Input;

use crate::ast::*;
use crate::span::Span;
use crate::token::{Token, TokenKind};

type PResult<'a, T> = nom::IResult<Tokens<'a>, T, ParseErr>;

/// トークン列をプログラムへ解析するエントリポイント。
pub fn parse(tokens: &[Token]) -> Result<Program, ParseErr> {
    let mut input = Tokens::new(tokens);
    let mut items = Vec::new();

    loop {
        input = skip_newlines(input);
        if input.peek() == &TokenKind::Eof {
            break;
        }
        let (rest, item) = unwrap(parse_item(input))?;
        items.push(item);
        input = rest;
    }

    Ok(Program { items })
}

/// nom の `Err` を素の `ParseErr` に変換する。complete 解析のため `Incomplete` は来ない。
fn unwrap<T>(result: PResult<T>) -> Result<(Tokens, T), ParseErr> {
    result.map_err(|e| match e {
        nom::Err::Error(e) | nom::Err::Failure(e) => e,
        nom::Err::Incomplete(_) => ParseErr::new(Span::new(0, 0), "入力が途中で終了しました"),
    })
}

// ---- 小さなヘルパ -------------------------------------------------------

/// 先頭の連続する改行を読み飛ばす。
fn skip_newlines(mut input: Tokens) -> Tokens {
    while input.peek() == &TokenKind::Newline {
        input = input.take_from(1);
    }
    input
}

/// 指定した種別のトークンを 1 つ消費する。失敗時は回復可能エラー。
fn eat<'a>(input: Tokens<'a>, want: &TokenKind, what: &str) -> PResult<'a, &'a Token> {
    let tok = input.first();
    if &tok.kind == want {
        Ok((input.take_from(1), tok))
    } else {
        Err(nom::Err::Error(ParseErr::expected(what, tok)))
    }
}

/// 指定した種別のトークンを 1 つ消費する。失敗時は回復不能エラー（`cut` 相当）。
fn expect<'a>(input: Tokens<'a>, want: &TokenKind, what: &str) -> PResult<'a, &'a Token> {
    let tok = input.first();
    if &tok.kind == want {
        Ok((input.take_from(1), tok))
    } else {
        Err(nom::Err::Failure(ParseErr::expected(what, tok)))
    }
}

/// 識別子を 1 つ消費し、その名前と span を返す。
fn ident(input: Tokens) -> PResult<(String, Span)> {
    let tok = input.first();
    if let TokenKind::Ident(name) = &tok.kind {
        Ok((input.take_from(1), (name.clone(), tok.span)))
    } else {
        Err(nom::Err::Error(ParseErr::expected("識別子", tok)))
    }
}

// ---- 宣言 ---------------------------------------------------------------

fn parse_item(input: Tokens) -> PResult<Item> {
    let (input, is_pub) = match eat(input, &TokenKind::Pub, "`pub`") {
        Ok((rest, _)) => (rest, true),
        Err(_) => (input, false),
    };

    let tok = input.first();
    match &tok.kind {
        TokenKind::Fn => {
            parse_function(input, is_pub, false, None).map(|(i, f)| (i, Item::Function(f)))
        }
        TokenKind::Extern => {
            let (input, _) = eat(input, &TokenKind::Extern, "`extern`")?;
            parse_function(input, is_pub, true, None).map(|(i, f)| (i, Item::Function(f)))
        }
        TokenKind::Type => parse_type_def(input, is_pub).map(|(i, t)| (i, Item::TypeDef(t))),
        // `impl Type { ... }`（固有メソッド）。`impl` は予約語ではなく識別子で扱う。
        TokenKind::Ident(name) if name == "impl" => parse_impl(input).map(|(i, m)| (i, Item::Impl(m))),
        _ => Err(nom::Err::Error(ParseErr::expected(
            "宣言 (`fn`・`extern fn`・`type`・`impl`)",
            input.first(),
        ))),
    }
}

/// `impl Type { メソッド... }` を解析する。メソッドは `fn ...`（`self` 可）。
fn parse_impl(input: Tokens) -> PResult<Impl> {
    let impl_kw = input.first();
    let start = impl_kw.span;
    let (input, (type_name, type_name_span)) = ident(input.take_from(1))?;
    let (mut input, _) = expect(input, &TokenKind::LBrace, "`{`")?;

    let mut methods = Vec::new();
    loop {
        input = skip_newlines(input);
        match input.peek() {
            TokenKind::RBrace => break,
            TokenKind::Eof => return Err(nom::Err::Failure(ParseErr::expected("`}`", input.first()))),
            _ => {}
        }
        // メソッドの可視性（省略可）。
        let (rest, is_pub) = match eat(input, &TokenKind::Pub, "`pub`") {
            Ok((r, _)) => (r, true),
            Err(_) => (input, false),
        };
        let (rest, method) = parse_function(rest, is_pub, false, Some((&type_name, type_name_span)))?;
        methods.push(method);
        input = rest;
    }
    let (input, rbrace) = expect(input, &TokenKind::RBrace, "`}`")?;
    let span = start.merge(rbrace.span);
    Ok((
        input,
        Impl {
            type_name,
            type_name_span,
            methods,
            span,
        },
    ))
}

// ---- 型定義 -------------------------------------------------------------

fn parse_type_def(input: Tokens, is_pub: bool) -> PResult<TypeDef> {
    let (input, type_kw) = eat(input, &TokenKind::Type, "`type`")?;
    let start = type_kw.span;
    let (input, (name, name_span)) = ident(input)?;
    let (input, generics) = parse_generics(input)?;
    let (input, _) = expect(input, &TokenKind::Assign, "`=`")?;

    let (input, body, end) = match input.peek() {
        TokenKind::Struct => {
            let (input, (fields, end)) = parse_struct_body(input.take_from(1))?;
            (input, TypeDefBody::Struct(fields), end)
        }
        TokenKind::Enum => {
            let (input, (variants, end)) = parse_enum_body(input.take_from(1))?;
            (input, TypeDefBody::Enum(variants), end)
        }
        _ => {
            let (input, ty) = parse_type(input)?;
            let end = ty.span();
            (input, TypeDefBody::Alias(ty), end)
        }
    };

    let span = start.merge(end);
    Ok((
        input,
        TypeDef {
            is_pub,
            name,
            name_span,
            generics,
            body,
            span,
        },
    ))
}

/// 省略可能な型パラメータ列 `<T, U>` を解析する。
fn parse_generics(input: Tokens) -> PResult<Vec<Generic>> {
    if input.peek() != &TokenKind::Lt {
        return Ok((input, Vec::new()));
    }
    let mut input = input.take_from(1);
    let mut generics = Vec::new();
    loop {
        let (rest, (name, span)) = ident(input)?;
        generics.push(Generic { name, span });
        input = rest;
        if input.peek() == &TokenKind::Comma {
            input = input.take_from(1);
        } else {
            break;
        }
    }
    let (input, _) = expect(input, &TokenKind::Gt, "`>`")?;
    Ok((input, generics))
}

/// `{ field... }` を解析する。フィールドは改行またはカンマで区切る。
/// 戻り値はフィールド列と閉じ `}` の span。
fn parse_struct_body(input: Tokens) -> PResult<(Vec<Field>, Span)> {
    let (mut input, _) = expect(input, &TokenKind::LBrace, "`{`")?;
    let mut fields = Vec::new();
    loop {
        input = skip_separators(input);
        if input.peek() == &TokenKind::RBrace {
            break;
        }
        let (rest, (name, name_span)) = ident(input)?;
        let (rest, _) = expect(rest, &TokenKind::Colon, "`:`")?;
        let (rest, ty) = parse_type(rest)?;
        let span = name_span.merge(ty.span());
        fields.push(Field { name, ty, span });
        input = require_separator(rest)?;
    }
    let (input, rbrace) = expect(input, &TokenKind::RBrace, "`}`")?;
    Ok((input, (fields, rbrace.span)))
}

/// `{ Variant... }` を解析する。バリアントは `Name` または `Name: Type`。
fn parse_enum_body(input: Tokens) -> PResult<(Vec<Variant>, Span)> {
    let (mut input, _) = expect(input, &TokenKind::LBrace, "`{`")?;
    let mut variants = Vec::new();
    loop {
        input = skip_separators(input);
        if input.peek() == &TokenKind::RBrace {
            break;
        }
        let (rest, (name, name_span)) = ident(input)?;
        let (rest, payload) = match eat(rest, &TokenKind::Colon, "`:`") {
            Ok((after, _)) => {
                let (after, ty) = parse_type(after)?;
                (after, Some(ty))
            }
            Err(_) => (rest, None),
        };
        let span = payload
            .as_ref()
            .map_or(name_span, |t| name_span.merge(t.span()));
        variants.push(Variant {
            name,
            payload,
            span,
        });
        input = require_separator(rest)?;
    }
    let (input, rbrace) = expect(input, &TokenKind::RBrace, "`}`")?;
    Ok((input, (variants, rbrace.span)))
}

/// 改行・カンマ（区切り）を読み飛ばす。
fn skip_separators(mut input: Tokens) -> Tokens {
    while matches!(input.peek(), TokenKind::Newline | TokenKind::Comma) {
        input = input.take_from(1);
    }
    input
}

/// 要素のあとに区切り（改行・カンマ・`}`）があることを確認する。
fn require_separator(input: Tokens) -> Result<Tokens, nom::Err<ParseErr>> {
    match input.peek() {
        TokenKind::Newline | TokenKind::Comma | TokenKind::RBrace => Ok(input),
        _ => Err(nom::Err::Failure(ParseErr::expected(
            "改行・`,`・`}`",
            input.first(),
        ))),
    }
}

/// 関数（メソッド）を解析する。`impl_type` が `Some` のときは impl ブロック内の
/// メソッドとして、先頭の `self` / `&self` / `&mut self` を受け付ける。
fn parse_function<'a>(
    input: Tokens<'a>,
    is_pub: bool,
    is_extern: bool,
    impl_type: Option<(&str, Span)>,
) -> PResult<'a, Function> {
    let (input, fn_kw) = eat(input, &TokenKind::Fn, "`fn`")?;
    let start = fn_kw.span;

    let (input, (name, name_span)) = ident(input)?;

    let (mut input, _) = expect(input, &TokenKind::LParen, "`(`")?;

    // パラメータ列。メソッドでは先頭に self を許す（`params` の先頭へ合成する）。
    let mut params = Vec::new();
    let mut self_kind = None;
    if let Some((ty_name, ty_span)) = impl_type
        && let Some((kind, self_param, rest)) = parse_self_param(input, ty_name, ty_span)
    {
        self_kind = Some(kind);
        params.push(self_param);
        input = rest;
        // self の後ろにカンマがあれば残りの引数へ。
        if input.peek() == &TokenKind::Comma {
            input = input.take_from(1);
        }
    }
    if input.peek() != &TokenKind::RParen {
        loop {
            let (rest, param) = parse_param(input)?;
            params.push(param);
            input = rest;
            match input.peek() {
                TokenKind::Comma => {
                    input = input.take_from(1);
                    // 末尾カンマを許容
                    if input.peek() == &TokenKind::RParen {
                        break;
                    }
                }
                _ => break,
            }
        }
    }
    let (input, _) = expect(input, &TokenKind::RParen, "`)`")?;

    // 戻り値型（省略可）
    let (input, ret) = match eat(input, &TokenKind::Colon, "`:`") {
        Ok((rest, _)) => {
            let (rest, ty) = parse_type(rest)?;
            (rest, Some(ty))
        }
        Err(_) => (input, None),
    };

    // extern は本体を持たない。空ブロックを合成する。
    let (input, body) = if is_extern {
        let end = ret.as_ref().map_or(name_span, Type::span);
        (
            input,
            Block {
                stmts: Vec::new(),
                span: end,
            },
        )
    } else {
        parse_block(input)?
    };
    let span = start.merge(body.span);

    Ok((
        input,
        Function {
            is_pub,
            is_extern,
            name,
            name_span,
            self_kind,
            params,
            ret,
            body,
            span,
        },
    ))
}

/// メソッド先頭の `self` / `&self` / `&mut self` を解析する。
/// 型は impl 対象の型から合成する（注釈は書かない）。self でなければ `None`。
fn parse_self_param<'a>(
    input: Tokens<'a>,
    ty_name: &str,
    ty_span: Span,
) -> Option<(SelfKind, Param, Tokens<'a>)> {
    let tok = input.first();
    let inner = || Type::Named {
        name: ty_name.to_string(),
        args: Vec::new(),
        span: ty_span,
    };
    match &tok.kind {
        // `self`（値）。`self: T` のような注釈付きは通常の引数として扱う。
        TokenKind::Ident(n) if n == "self" && input.take_from(1).peek() != &TokenKind::Colon => {
            let param = Param {
                name: "self".to_string(),
                ty: inner(),
                span: tok.span,
            };
            Some((SelfKind::Value, param, input.take_from(1)))
        }
        // `&self` / `&mut self`。
        TokenKind::Amp => {
            let after = input.take_from(1);
            let (after, mutable) = if after.peek() == &TokenKind::Mut {
                (after.take_from(1), true)
            } else {
                (after, false)
            };
            let self_tok = after.first();
            match &self_tok.kind {
                TokenKind::Ident(n) if n == "self" => {
                    let span = tok.span.merge(self_tok.span);
                    let param = Param {
                        name: "self".to_string(),
                        ty: Type::Ref {
                            mutable,
                            inner: Box::new(inner()),
                            span,
                        },
                        span,
                    };
                    let kind = if mutable { SelfKind::RefMut } else { SelfKind::Ref };
                    Some((kind, param, after.take_from(1)))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn parse_param(input: Tokens) -> PResult<Param> {
    let (input, (name, name_span)) = ident(input)?;
    let (input, _) = expect(input, &TokenKind::Colon, "`:`")?;
    let (input, ty) = parse_type(input)?;
    let span = name_span.merge(ty.span());
    Ok((input, Param { name, ty, span }))
}

// ---- 型 -----------------------------------------------------------------

fn parse_type(input: Tokens) -> PResult<Type> {
    let (mut input, mut ty) = parse_type_base(input)?;
    // 配列サフィックス `[]`
    while input.peek() == &TokenKind::LBracket {
        let lb = input.first().span;
        let after_lb = input.take_from(1);
        let (rest, rb) = expect(after_lb, &TokenKind::RBracket, "`]`")?;
        let span = ty.span().merge(lb.merge(rb.span));
        ty = Type::Array {
            inner: Box::new(ty),
            span,
        };
        input = rest;
    }
    Ok((input, ty))
}

fn parse_type_base(input: Tokens) -> PResult<Type> {
    let tok = input.first();
    match &tok.kind {
        TokenKind::Amp => {
            let start = tok.span;
            let input = input.take_from(1);
            let (input, mutable) = match eat(input, &TokenKind::Mut, "`mut`") {
                Ok((rest, _)) => (rest, true),
                Err(_) => (input, false),
            };
            let (input, inner) = parse_type(input)?;
            let span = start.merge(inner.span());
            Ok((
                input,
                Type::Ref {
                    mutable,
                    inner: Box::new(inner),
                    span,
                },
            ))
        }
        TokenKind::LParen => {
            let start = tok.span;
            let mut input = input.take_from(1);
            let mut elems = Vec::new();
            if input.peek() != &TokenKind::RParen {
                loop {
                    let (rest, t) = parse_type(input)?;
                    elems.push(t);
                    input = rest;
                    if input.peek() == &TokenKind::Comma {
                        input = input.take_from(1);
                        if input.peek() == &TokenKind::RParen {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            let (input, rp) = expect(input, &TokenKind::RParen, "`)`")?;
            let span = start.merge(rp.span);
            Ok((input, Type::Tuple { elems, span }))
        }
        TokenKind::Ident(name) => {
            let name = name.clone();
            let start = tok.span;
            let mut input = input.take_from(1);
            let mut end = start;
            let mut args = Vec::new();
            // ジェネリック引数 `<T, U>`
            if input.peek() == &TokenKind::Lt {
                input = input.take_from(1);
                loop {
                    let (rest, t) = parse_type(input)?;
                    args.push(t);
                    input = rest;
                    if input.peek() == &TokenKind::Comma {
                        input = input.take_from(1);
                    } else {
                        break;
                    }
                }
                let (rest, gt) = expect(input, &TokenKind::Gt, "`>`")?;
                end = gt.span;
                input = rest;
            }
            let span = start.merge(end);
            Ok((input, Type::Named { name, args, span }))
        }
        _ => Err(nom::Err::Error(ParseErr::expected("型", tok))),
    }
}

// ---- 文 -----------------------------------------------------------------

fn parse_block(input: Tokens) -> PResult<Block> {
    let (mut input, lbrace) = expect(input, &TokenKind::LBrace, "`{`")?;
    let start = lbrace.span;
    let mut stmts = Vec::new();

    loop {
        input = skip_newlines(input);
        match input.peek() {
            TokenKind::RBrace => break,
            TokenKind::Eof => {
                return Err(nom::Err::Failure(ParseErr::expected("`}`", input.first())));
            }
            _ => {}
        }

        let (rest, stmt) = parse_stmt(input)?;
        stmts.push(stmt);
        input = rest;

        // 文の区切りは改行または `}`
        match input.peek() {
            TokenKind::Newline | TokenKind::RBrace => {}
            _ => {
                return Err(nom::Err::Failure(ParseErr::expected(
                    "改行または `}`",
                    input.first(),
                )));
            }
        }
    }

    let (input, rbrace) = expect(input, &TokenKind::RBrace, "`}`")?;
    let span = start.merge(rbrace.span);
    Ok((input, Block { stmts, span }))
}

fn parse_stmt(input: Tokens) -> PResult<Stmt> {
    match input.peek() {
        TokenKind::Let | TokenKind::Const => parse_let(input),
        TokenKind::Return => parse_return(input),
        TokenKind::While => parse_while(input),
        TokenKind::Loop => parse_loop(input),
        TokenKind::Break => {
            let span = input.first().span;
            Ok((input.take_from(1), Stmt::Break { span }))
        }
        TokenKind::Continue => {
            let span = input.first().span;
            Ok((input.take_from(1), Stmt::Continue { span }))
        }
        _ => {
            let (input, expr) = parse_expr(input)?;
            // 再代入 `target = value`
            if input.peek() == &TokenKind::Assign {
                let (input, value) = parse_expr(input.take_from(1))?;
                let span = expr.span.merge(value.span);
                Ok((
                    input,
                    Stmt::Assign {
                        target: expr,
                        value,
                        span,
                    },
                ))
            } else {
                Ok((input, Stmt::Expr(expr)))
            }
        }
    }
}

fn parse_let(input: Tokens) -> PResult<Stmt> {
    let kw = input.first();
    let is_const = kw.kind == TokenKind::Const;
    let start = kw.span;
    let input = input.take_from(1);

    // `mut`（const には付けられない）
    let (input, mutable) = if !is_const {
        match eat(input, &TokenKind::Mut, "`mut`") {
            Ok((rest, _)) => (rest, true),
            Err(_) => (input, false),
        }
    } else {
        (input, false)
    };

    let (input, (name, _)) = ident(input)?;

    // 型注釈（省略可）
    let (input, ty) = match eat(input, &TokenKind::Colon, "`:`") {
        Ok((rest, _)) => {
            let (rest, t) = parse_type(rest)?;
            (rest, Some(t))
        }
        Err(_) => (input, None),
    };

    let (input, _) = expect(input, &TokenKind::Assign, "`=`")?;
    let (input, value) = parse_expr(input)?;
    let span = start.merge(value.span);

    Ok((
        input,
        Stmt::Let {
            is_const,
            mutable,
            name,
            ty,
            value,
            span,
        },
    ))
}

fn parse_return(input: Tokens) -> PResult<Stmt> {
    let kw = input.first();
    let start = kw.span;
    let input = input.take_from(1);

    // 戻り値の有無は次トークンで判断する。
    match input.peek() {
        TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof => Ok((
            input,
            Stmt::Return {
                value: None,
                span: start,
            },
        )),
        _ => {
            let (input, expr) = parse_expr(input)?;
            let span = start.merge(expr.span);
            Ok((
                input,
                Stmt::Return {
                    value: Some(expr),
                    span,
                },
            ))
        }
    }
}

fn parse_while(input: Tokens) -> PResult<Stmt> {
    let kw = input.first();
    let start = kw.span;
    // 条件式ではブロックとの曖昧性を避けるため構造体リテラルを禁止する。
    let (input, cond) = parse_expr_r(input.take_from(1), true)?;
    let (input, body) = parse_block(input)?;
    let span = start.merge(body.span);
    Ok((input, Stmt::While { cond, body, span }))
}

fn parse_loop(input: Tokens) -> PResult<Stmt> {
    let kw = input.first();
    let start = kw.span;
    let (input, body) = parse_block(input.take_from(1))?;
    let span = start.merge(body.span);
    Ok((input, Stmt::Loop { body, span }))
}

// ---- 式 -----------------------------------------------------------------

fn parse_expr(input: Tokens) -> PResult<Expr> {
    parse_expr_r(input, false)
}

/// 式を解析する。`no_struct` が真の間は、トップレベルで構造体リテラル
/// `Name { ... }` を解析しない（`if cond { ... }` のブロックとの曖昧性回避）。
fn parse_expr_r(input: Tokens, no_struct: bool) -> PResult<Expr> {
    parse_ternary(input, no_struct)
}

fn parse_ternary(input: Tokens, no_struct: bool) -> PResult<Expr> {
    let (input, cond) = parse_or(input, no_struct)?;
    if input.peek() == &TokenKind::Question {
        let input = input.take_from(1);
        let (input, then) = parse_expr_r(input, no_struct)?;
        let (input, _) = expect(input, &TokenKind::Colon, "`:`")?;
        let (input, otherwise) = parse_expr_r(input, no_struct)?;
        let span = cond.span.merge(otherwise.span);
        Ok((
            input,
            Expr {
                kind: ExprKind::Ternary {
                    cond: Box::new(cond),
                    then: Box::new(then),
                    otherwise: Box::new(otherwise),
                },
                span,
            },
        ))
    } else {
        Ok((input, cond))
    }
}

/// 左結合の二項演算子の段を解析する共通ロジック。
fn binary_level<'a>(
    input: Tokens<'a>,
    ops: &[(TokenKind, BinaryOp)],
    next: fn(Tokens<'a>, bool) -> PResult<'a, Expr>,
    no_struct: bool,
) -> PResult<'a, Expr> {
    let (mut input, mut lhs) = next(input, no_struct)?;
    loop {
        let kind = input.peek();
        let matched = ops.iter().find(|(k, _)| k == kind).map(|(_, op)| *op);
        match matched {
            Some(op) => {
                let (rest, rhs) = next(input.take_from(1), no_struct)?;
                let span = lhs.span.merge(rhs.span);
                lhs = Expr {
                    kind: ExprKind::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    span,
                };
                input = rest;
            }
            None => break,
        }
    }
    Ok((input, lhs))
}

fn parse_or(input: Tokens, no_struct: bool) -> PResult<Expr> {
    binary_level(input, &[(TokenKind::OrOr, BinaryOp::Or)], parse_and, no_struct)
}

fn parse_and(input: Tokens, no_struct: bool) -> PResult<Expr> {
    binary_level(
        input,
        &[(TokenKind::AndAnd, BinaryOp::And)],
        parse_equality,
        no_struct,
    )
}

fn parse_equality(input: Tokens, no_struct: bool) -> PResult<Expr> {
    binary_level(
        input,
        &[
            (TokenKind::EqEq, BinaryOp::Eq),
            (TokenKind::NotEq, BinaryOp::NotEq),
        ],
        parse_comparison,
        no_struct,
    )
}

fn parse_comparison(input: Tokens, no_struct: bool) -> PResult<Expr> {
    binary_level(
        input,
        &[
            (TokenKind::Lt, BinaryOp::Lt),
            (TokenKind::LtEq, BinaryOp::LtEq),
            (TokenKind::Gt, BinaryOp::Gt),
            (TokenKind::GtEq, BinaryOp::GtEq),
        ],
        parse_additive,
        no_struct,
    )
}

fn parse_additive(input: Tokens, no_struct: bool) -> PResult<Expr> {
    binary_level(
        input,
        &[
            (TokenKind::Plus, BinaryOp::Add),
            (TokenKind::Minus, BinaryOp::Sub),
        ],
        parse_multiplicative,
        no_struct,
    )
}

fn parse_multiplicative(input: Tokens, no_struct: bool) -> PResult<Expr> {
    binary_level(
        input,
        &[
            (TokenKind::Star, BinaryOp::Mul),
            (TokenKind::Slash, BinaryOp::Div),
            (TokenKind::Percent, BinaryOp::Rem),
        ],
        parse_unary,
        no_struct,
    )
}

fn parse_unary(input: Tokens, no_struct: bool) -> PResult<Expr> {
    let tok = input.first();
    match tok.kind {
        TokenKind::Minus => {
            let start = tok.span;
            let (input, expr) = parse_unary(input.take_from(1), no_struct)?;
            let span = start.merge(expr.span);
            Ok((
                input,
                Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    },
                    span,
                },
            ))
        }
        TokenKind::Amp => {
            let start = tok.span;
            let input = input.take_from(1);
            let (input, mutable) = match eat(input, &TokenKind::Mut, "`mut`") {
                Ok((rest, _)) => (rest, true),
                Err(_) => (input, false),
            };
            let (input, expr) = parse_unary(input, no_struct)?;
            let span = start.merge(expr.span);
            let op = if mutable { UnaryOp::RefMut } else { UnaryOp::Ref };
            Ok((
                input,
                Expr {
                    kind: ExprKind::Unary {
                        op,
                        expr: Box::new(expr),
                    },
                    span,
                },
            ))
        }
        _ => parse_postfix(input, no_struct),
    }
}

fn parse_postfix(input: Tokens, no_struct: bool) -> PResult<Expr> {
    let (mut input, mut expr) = parse_primary(input, no_struct)?;
    loop {
        match input.peek() {
            // 関数呼び出し
            TokenKind::LParen => {
                let mut cur = input.take_from(1);
                let mut args = Vec::new();
                if cur.peek() != &TokenKind::RParen {
                    loop {
                        let (rest, arg) = parse_expr(cur)?;
                        args.push(arg);
                        cur = rest;
                        if cur.peek() == &TokenKind::Comma {
                            cur = cur.take_from(1);
                            if cur.peek() == &TokenKind::RParen {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                let (rest, rp) = expect(cur, &TokenKind::RParen, "`)`")?;
                let span = expr.span.merge(rp.span);
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                };
                input = rest;
            }
            // メンバアクセス
            TokenKind::Dot => {
                let after_dot = input.take_from(1);
                let (rest, (field, field_span)) = ident(after_dot)?;
                let span = expr.span.merge(field_span);
                expr = Expr {
                    kind: ExprKind::Member {
                        object: Box::new(expr),
                        field,
                    },
                    span,
                };
                input = rest;
            }
            // エラー伝播 `!`
            TokenKind::Bang => {
                let bang_span = input.first().span;
                let span = expr.span.merge(bang_span);
                expr = Expr {
                    kind: ExprKind::Try(Box::new(expr)),
                    span,
                };
                input = input.take_from(1);
            }
            _ => break,
        }
    }
    Ok((input, expr))
}

fn parse_primary(input: Tokens, no_struct: bool) -> PResult<Expr> {
    let tok = input.first();
    let span = tok.span;
    match &tok.kind {
        TokenKind::Int(v) => Ok((
            input.take_from(1),
            Expr {
                kind: ExprKind::Int(*v),
                span,
            },
        )),
        TokenKind::Float(v) => Ok((
            input.take_from(1),
            Expr {
                kind: ExprKind::Float(*v),
                span,
            },
        )),
        TokenKind::Str(s) => Ok((
            input.take_from(1),
            Expr {
                kind: ExprKind::Str(s.clone()),
                span,
            },
        )),
        TokenKind::Bool(b) => Ok((
            input.take_from(1),
            Expr {
                kind: ExprKind::Bool(*b),
                span,
            },
        )),
        TokenKind::Ident(name) => {
            let name = name.clone();
            let rest = input.take_from(1);
            // 構造体リテラル `Name { ... }`。条件位置（no_struct）では解析しない。
            if !no_struct && rest.peek() == &TokenKind::LBrace {
                parse_struct_lit(name, span, rest)
            } else {
                Ok((
                    rest,
                    Expr {
                        kind: ExprKind::Ident(name),
                        span,
                    },
                ))
            }
        }
        TokenKind::LParen => {
            let (input, expr) = parse_expr(input.take_from(1))?;
            let (input, _) = expect(input, &TokenKind::RParen, "`)`")?;
            Ok((input, expr))
        }
        TokenKind::If => parse_if(input),
        _ => Err(nom::Err::Error(ParseErr::expected("式", tok))),
    }
}

fn parse_if(input: Tokens) -> PResult<Expr> {
    let if_kw = input.first();
    let start = if_kw.span;
    // 条件式ではブロックとの曖昧性を避けるため構造体リテラルを禁止する。
    let (input, cond) = parse_expr_r(input.take_from(1), true)?;
    let (input, then) = parse_block(input)?;

    let (input, otherwise, end) = if input.peek() == &TokenKind::Else {
        let input = input.take_from(1);
        if input.peek() == &TokenKind::If {
            let (input, else_if) = parse_if(input)?;
            let end = else_if.span;
            (input, Some(Box::new(Else::If(else_if))), end)
        } else {
            let (input, block) = parse_block(input)?;
            let end = block.span;
            (input, Some(Box::new(Else::Block(block))), end)
        }
    } else {
        (input, None, then.span)
    };

    let span = start.merge(end);
    Ok((
        input,
        Expr {
            kind: ExprKind::If {
                cond: Box::new(cond),
                then,
                otherwise,
            },
            span,
        },
    ))
}

/// 構造体リテラル `Name { field: value, ... }` を解析する。
/// `input` は `{` を指している。
fn parse_struct_lit(name: String, name_span: Span, input: Tokens) -> PResult<Expr> {
    let (mut input, _) = expect(input, &TokenKind::LBrace, "`{`")?;
    let mut fields = Vec::new();
    loop {
        input = skip_separators(input);
        if input.peek() == &TokenKind::RBrace {
            break;
        }
        let (rest, (fname, fname_span)) = ident(input)?;
        let (rest, _) = expect(rest, &TokenKind::Colon, "`:`")?;
        let (rest, value) = parse_expr(rest)?;
        let span = fname_span.merge(value.span);
        fields.push(FieldInit {
            name: fname,
            name_span: fname_span,
            value,
            span,
        });
        input = require_separator(rest)?;
    }
    let (input, rbrace) = expect(input, &TokenKind::RBrace, "`}`")?;
    let span = name_span.merge(rbrace.span);
    Ok((
        input,
        Expr {
            kind: ExprKind::StructLit {
                name,
                name_span,
                fields,
            },
            span,
        },
    ))
}
