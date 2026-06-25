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
        // `impl [Trait for] Type { ... }`。`impl` は予約語ではなく識別子で扱う。
        TokenKind::Ident(name) if name == "impl" => parse_impl(input).map(|(i, m)| (i, Item::Impl(m))),
        // `trait Name { ... }`。`trait` も識別子で扱う。
        TokenKind::Ident(name) if name == "trait" => {
            parse_trait(input, is_pub).map(|(i, t)| (i, Item::Trait(t)))
        }
        TokenKind::Use => parse_use_item(input).map(|(i, u)| (i, Item::Use(u))),
        _ => Err(nom::Err::Error(ParseErr::expected(
            "宣言 (`fn`・`extern fn`・`type`・`trait`・`impl`・`use`)",
            input.first(),
        ))),
    }
}

/// `use a.b.c [{ x, y } | .*]` を解析する。
///
/// - `use a.b.c`         → `UseTree::Plain`
/// - `use a.b.c { x, y }` → `UseTree::Named(["x", "y"])`
/// - `use a.b.c.*`       → `UseTree::Glob`
fn parse_use_item(input: Tokens) -> PResult<UseDecl> {
    let start = input.first().span;
    let (mut input, _) = eat(input, &TokenKind::Use, "`use`")?;

    // パス: `a.b.c`（ドット区切りの識別子列）
    let (rest, (first_seg, _)) = ident(input)?;
    input = rest;
    let mut path = vec![first_seg];
    // ドットが続く限り次のセグメントを読む。ただし `.*` の `*` は識別子でないので先に確認する。
    loop {
        // `.` の後に `*` が来るなら Glob、識別子が来るならパスセグメント。
        if input.peek() != &TokenKind::Dot {
            break;
        }
        // Dot を消費してから次を見る。
        let after_dot = input.take_from(1);
        match after_dot.peek() {
            TokenKind::Star => {
                // `.*` → Glob。
                let end = after_dot.first().span;
                input = after_dot.take_from(1);
                let span = start.merge(end);
                return Ok((input, UseDecl { path, tree: UseTree::Glob, span }));
            }
            TokenKind::Ident(_) => {
                let (rest, (seg, _)) = ident(after_dot)?;
                path.push(seg);
                input = rest;
            }
            _ => break,
        }
    }

    // `{ x, y }` の選択インポート。
    if input.peek() == &TokenKind::LBrace {
        input = input.take_from(1);
        let mut names = Vec::new();
        loop {
            input = skip_newlines(input);
            match input.peek() {
                TokenKind::RBrace | TokenKind::Eof => break,
                _ => {}
            }
            let (rest, (name, _)) = ident(input)?;
            names.push(name);
            input = rest;
            // カンマまたは改行で区切る。
            match input.peek() {
                TokenKind::Comma => { input = input.take_from(1); }
                TokenKind::Newline => { input = input.take_from(1); }
                _ => {}
            }
        }
        let (rest, rbrace) = expect(input, &TokenKind::RBrace, "`}`")?;
        let span = start.merge(rbrace.span);
        return Ok((rest, UseDecl { path, tree: UseTree::Named(names), span }));
    }

    // Plain import。
    let span = start.merge(input.first().span);
    Ok((input, UseDecl { path, tree: UseTree::Plain, span }))
}

/// `impl [Trait for] Type { メソッド... }` を解析する。メソッドは `fn ...`（`self` 可）。
/// 先頭の `Name<Args>` を読み、続く `for` の有無で固有 impl / `impl Trait for Type` を判別する。
fn parse_impl(input: Tokens) -> PResult<Impl> {
    let start = input.first().span;
    // 先頭の名前（トレイトか型かはこの時点で未確定）。
    let (input, first) = parse_trait_ref(input.take_from(1))?;
    // `for` が続けば `impl Trait for Type`、なければ固有 `impl Type`。
    // `for` は `for x in ...` の予約語と同じトークン（文脈キーワードとして流用）。
    let is_for = matches!(input.peek(), TokenKind::For);
    let (input, trait_ref, type_name, type_name_span) = if is_for {
        let (input, (tname, tspan)) = ident(input.take_from(1))?;
        (input, Some(first), tname, tspan)
    } else {
        let span = first.span;
        (input, None, first.name, span)
    };

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
            trait_ref,
            type_name,
            type_name_span,
            methods,
            span,
        },
    ))
}

/// `trait Name<T>: Super { メソッド... }` を解析する。
/// メソッドはシグネチャのみ、または既定実装（`{ ... }` 付き）。
fn parse_trait(input: Tokens, is_pub: bool) -> PResult<TraitDef> {
    let start = input.first().span; // `trait`
    let (input, (name, name_span)) = ident(input.take_from(1))?;
    let (input, generics) = parse_generics(input)?;
    // スーパートレイト `: Super + Show`
    let (mut input, supertraits) = if input.peek() == &TokenKind::Colon {
        let (rest, bs) = parse_bounds(input.take_from(1))?;
        (rest, bs)
    } else {
        (input, Vec::new())
    };
    input = skip_newlines(input);
    input = expect(input, &TokenKind::LBrace, "`{`")?.0;

    let mut methods = Vec::new();
    loop {
        input = skip_newlines(input);
        match input.peek() {
            TokenKind::RBrace => break,
            TokenKind::Eof => return Err(nom::Err::Failure(ParseErr::expected("`}`", input.first()))),
            _ => {}
        }
        // トレイトメソッド: `fn ...`。本体があれば既定実装、なければシグネチャのみ。
        // self の型は実装型（`Self`）として合成する。
        let (rest, (func, had_body)) =
            parse_function_opt_body(input, false, false, Some(("Self", name_span)), true)?;
        methods.push(TraitMethod {
            func,
            default: had_body,
        });
        input = rest;
    }
    let (input, rbrace) = expect(input, &TokenKind::RBrace, "`}`")?;
    Ok((
        input,
        TraitDef {
            is_pub,
            name,
            name_span,
            generics,
            supertraits,
            methods,
            span: start.merge(rbrace.span),
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

/// 省略可能な型パラメータ列 `<T, U: Bound + Bound>` を解析する。
fn parse_generics(input: Tokens) -> PResult<Vec<Generic>> {
    if input.peek() != &TokenKind::Lt {
        return Ok((input, Vec::new()));
    }
    let mut input = input.take_from(1);
    let mut generics = Vec::new();
    loop {
        let (rest, (name, span)) = ident(input)?;
        input = rest;
        // 省略可能なトレイト境界 `: Bound + Bound`
        let mut bounds = Vec::new();
        if input.peek() == &TokenKind::Colon {
            let (rest, bs) = parse_bounds(input.take_from(1))?;
            bounds = bs;
            input = rest;
        }
        generics.push(Generic { name, bounds, span });
        if input.peek() == &TokenKind::Comma {
            input = input.take_from(1);
        } else {
            break;
        }
    }
    let (input, _) = expect(input, &TokenKind::Gt, "`>`")?;
    Ok((input, generics))
}

/// トレイト参照 `Name<Args>` を解析する（`impl Trait for`・スーパートレイト・境界・修飾子で共通）。
fn parse_trait_ref(input: Tokens) -> PResult<TraitRef> {
    let (mut input, (name, start)) = ident(input)?;
    let mut end = start;
    let mut args = Vec::new();
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
    Ok((
        input,
        TraitRef {
            name,
            args,
            span: start.merge(end),
        },
    ))
}

/// 1 個以上のトレイト境界を `+` 区切りで解析する（`: A + B` の `:` は呼び出し側が消費済み）。
fn parse_bounds(input: Tokens) -> PResult<Vec<TraitRef>> {
    let (mut input, first) = parse_trait_ref(input)?;
    let mut bounds = vec![first];
    while input.peek() == &TokenKind::Plus {
        let (rest, b) = parse_trait_ref(input.take_from(1))?;
        bounds.push(b);
        input = rest;
    }
    Ok((input, bounds))
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
        // `Name: Type` または `Name(Type)` でペイロード型を指定できる。
        let (rest, payload) = if eat(rest, &TokenKind::Colon, "`:`").is_ok() {
            let (after, _) = eat(rest, &TokenKind::Colon, "`:`").unwrap();
            let (after, ty) = parse_type(after)?;
            (after, Some(ty))
        } else if rest.peek() == &TokenKind::LParen {
            let rest = rest.take_from(1);
            let (rest, ty) = parse_type(rest)?;
            let (rest, _) = expect(rest, &TokenKind::RParen, "`)`")?;
            (rest, Some(ty))
        } else {
            (rest, None)
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
    let (input, (func, _)) = parse_function_opt_body(input, is_pub, is_extern, impl_type, false)?;
    Ok((input, func))
}

/// 関数本体。`body_optional` が true で本体（`{`）が無ければシグネチャのみとして空本体を合成し、
/// `had_body=false` を返す（トレイトのシグネチャ専用メソッドに使う）。
/// 戻り値の bool は本体（既定実装）が存在したか。
fn parse_function_opt_body<'a>(
    input: Tokens<'a>,
    is_pub: bool,
    is_extern: bool,
    impl_type: Option<(&str, Span)>,
    body_optional: bool,
) -> PResult<'a, (Function, bool)> {
    let (input, fn_kw) = eat(input, &TokenKind::Fn, "`fn`")?;
    let start = fn_kw.span;

    let (input, (name, name_span)) = ident(input)?;

    // 型パラメータ `<T: Bound>`（省略可）。
    let (input, mut generics) = parse_generics(input)?;

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

    // 引数位置の匿名トレイト境界 `x: A + B`（ADR-0006）を、新規ジェネリックパラメータへ脱糖する。
    for (i, p) in params.iter_mut().enumerate() {
        if let Type::Bound { bounds, span } = p.ty.clone() {
            let g_name = format!("__Bound{i}");
            generics.push(Generic {
                name: g_name.clone(),
                bounds,
                span,
            });
            p.ty = Type::Named {
                name: g_name,
                args: Vec::new(),
                span,
            };
        }
    }

    // 本体。extern は本体なし。body_optional でシグネチャのみも許す。
    let no_brace = input.peek() != &TokenKind::LBrace;
    let (input, body, had_body) = if is_extern || (body_optional && no_brace) {
        let end = ret.as_ref().map_or(name_span, Type::span);
        (
            input,
            Block {
                stmts: Vec::new(),
                span: end,
            },
            false,
        )
    } else {
        let (rest, b) = parse_block(input)?;
        (rest, b, true)
    };
    let span = start.merge(body.span);

    Ok((
        input,
        (
            Function {
                is_pub,
                is_extern,
                name,
                name_span,
                generics,
                self_kind,
                params,
                ret,
                body,
                span,
            },
            had_body,
        ),
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

/// 型を解析する。`allow_bounds` が真のとき `A + B` 形式の匿名トレイト境界も受け入れる。
/// `as` キャスト位置では `+` が加算演算子と曖昧になるため `allow_bounds=false` で呼ぶ。
fn parse_type_r(input: Tokens, allow_bounds: bool) -> PResult<Type> {
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
    // 匿名トレイト境界 `A + B`（ADR-0006、主に引数位置）。
    // キャスト位置（allow_bounds=false）では `+` を加算演算子として残す。
    if allow_bounds && input.peek() == &TokenKind::Plus {
        let first = named_to_trait_ref(ty)?;
        let start = first.span;
        let mut bounds = vec![first];
        let mut end = start;
        while input.peek() == &TokenKind::Plus {
            let (rest, tr) = parse_trait_ref(input.take_from(1))?;
            end = tr.span;
            bounds.push(tr);
            input = rest;
        }
        ty = Type::Bound {
            bounds,
            span: start.merge(end),
        };
    }
    Ok((input, ty))
}

fn parse_type(input: Tokens) -> PResult<Type> {
    parse_type_r(input, true)
}

/// `Type::Named` をトレイト参照へ変換する（匿名境界 `A + B` の各項用）。
fn named_to_trait_ref(ty: Type) -> Result<TraitRef, nom::Err<ParseErr>> {
    match ty {
        Type::Named { name, args, span } => Ok(TraitRef { name, args, span }),
        other => Err(nom::Err::Failure(ParseErr::new(
            other.span(),
            "トレイト境界には型名のみ書けます",
        ))),
    }
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
        TokenKind::For => parse_for(input),
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

/// `for x in start..end { ... }` / `for x in start..=end { ... }`。
/// 範囲はパターンと同じ規約（`..` は上限排他・`..=` は包含）。境界式・本体ともに
/// 構造体リテラルを抑制し、末尾の `{` をブロックとして取り扱う。
fn parse_for(input: Tokens) -> PResult<Stmt> {
    let kw = input.first();
    let kw_span = kw.span;
    let (input, (var, var_span)) = ident(input.take_from(1))?;
    let (input, _) = expect(input, &TokenKind::In, "`in`")?;
    let (input, lo) = parse_expr_r(input, true)?;
    // `..`（排他）か `..=`（包含）なら整数範囲 `for`。それ以外は一般イテレータ
    // `for x in iter`（lo がイテレータ式）。
    let (input, inclusive) = match input.peek() {
        TokenKind::DotDotEq => (input.take_from(1), true),
        TokenKind::DotDot => (input.take_from(1), false),
        _ => {
            let (input, body) = parse_block(input)?;
            let span = kw_span.merge(body.span);
            return Ok((
                input,
                Stmt::ForIn {
                    var,
                    var_span,
                    iter: lo,
                    body,
                    span,
                },
            ));
        }
    };
    let (input, hi) = parse_expr_r(input, true)?;
    let (input, body) = parse_block(input)?;
    let span = kw_span.merge(body.span);
    Ok((
        input,
        Stmt::For {
            var,
            var_span,
            start: lo,
            end: hi,
            inclusive,
            body,
            span,
        },
    ))
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
        parse_cast,
        no_struct,
    )
}

/// 型変換 `expr as Type`。二項演算子より強く・単項/後置より弱く結合する
/// （`a + b as T` は `a + (b as T)`、`-x as T` は `(-x) as T`）。左結合で連鎖も許す。
fn parse_cast(input: Tokens, no_struct: bool) -> PResult<Expr> {
    let (mut input, mut expr) = parse_unary(input, no_struct)?;
    while input.peek() == &TokenKind::As {
        // キャスト位置では `+` を加算演算子として残す（allow_bounds=false）。
        let (rest, ty) = parse_type_r(input.take_from(1), false)?;
        let span = expr.span.merge(ty.span());
        expr = Expr {
            kind: ExprKind::Cast {
                expr: Box::new(expr),
                ty,
            },
            span,
        };
        input = rest;
    }
    Ok((input, expr))
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
            // メンバアクセス（メソッド呼び出しの被メンバを含む）。
            // `.field#Trait<Args>` の修飾子（ADR-0004）を省略可で受ける。
            TokenKind::Dot => {
                let after_dot = input.take_from(1);
                let (rest, (field, field_span)) = ident(after_dot)?;
                let (rest, qualifier, end_span) = if rest.peek() == &TokenKind::Hash {
                    let (r, tr) = parse_trait_ref(rest.take_from(1))?;
                    let sp = tr.span;
                    (r, Some(tr), sp)
                } else {
                    (rest, None, field_span)
                };
                let span = expr.span.merge(end_span);
                expr = Expr {
                    kind: ExprKind::Member {
                        object: Box::new(expr),
                        field,
                        qualifier,
                    },
                    span,
                };
                input = rest;
            }
            // 添字アクセス `base[index]`（文字列・配列・Vec の要素読み）
            TokenKind::LBracket => {
                let (rest, index) = parse_expr(input.take_from(1))?;
                let (rest, rb) = expect(rest, &TokenKind::RBracket, "`]`")?;
                let span = expr.span.merge(rb.span);
                expr = Expr {
                    kind: ExprKind::Index {
                        base: Box::new(expr),
                        index: Box::new(index),
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
        // 配列リテラル `[e1, e2, ...]`（型注釈で `T[]` / `Vec<T>` のどちらにもなる）。
        // 要素は改行またはカンマ区切り。末尾カンマ・空リスト `[]` を許す。
        TokenKind::LBracket => {
            let lb_span = span;
            let mut cur = input.take_from(1);
            let mut elems = Vec::new();
            loop {
                cur = skip_separators(cur);
                if cur.peek() == &TokenKind::RBracket {
                    break;
                }
                let (rest, e) = parse_expr(cur)?;
                elems.push(e);
                cur = rest;
                match cur.peek() {
                    TokenKind::Comma | TokenKind::Newline | TokenKind::RBracket => {}
                    _ => {
                        return Err(nom::Err::Failure(ParseErr::expected(
                            "改行・`,`・`]`",
                            cur.first(),
                        )));
                    }
                }
            }
            let (cur, rb) = expect(cur, &TokenKind::RBracket, "`]`")?;
            let span = lb_span.merge(rb.span);
            Ok((
                cur,
                Expr {
                    kind: ExprKind::ArrayLit { elems },
                    span,
                },
            ))
        }
        TokenKind::If => parse_if(input),
        TokenKind::Match => parse_match(input),
        _ => Err(nom::Err::Error(ParseErr::expected("式", tok))),
    }
}

/// `match scrutinee { arm... }` を解析する。
/// アームは `pattern -> expr` の形で、改行で区切る。
fn parse_match(input: Tokens) -> PResult<Expr> {
    let kw = input.first();
    let start = kw.span;
    // scrutinee では構造体リテラルを禁じる（`{` との曖昧性）。
    let (input, scrutinee) = parse_expr_r(input.take_from(1), true)?;
    let (mut input, _) = expect(input, &TokenKind::LBrace, "`{`")?;

    let mut arms = Vec::new();
    loop {
        input = skip_newlines(input);
        match input.peek() {
            TokenKind::RBrace | TokenKind::Eof => break,
            _ => {}
        }
        let arm_start = input.first().span;
        let (rest, pat) = parse_pattern(input)?;
        // ガード `if cond`（省略可）。条件位置では構造体リテラルを禁じる。
        let (rest, guard) = if rest.peek() == &TokenKind::If {
            let (rest, cond) = parse_expr_r(rest.take_from(1), true)?;
            (rest, Some(cond))
        } else {
            (rest, None)
        };
        let (rest, _) = expect(rest, &TokenKind::Arrow, "`->`")?;
        let (rest, body) = parse_expr(rest)?;
        let arm_span = arm_start.merge(body.span);
        arms.push(MatchArm { pattern: pat, guard, body, span: arm_span });
        input = rest;
        // アームの区切りは改行・カンマ・`}`。
        match input.peek() {
            TokenKind::Newline | TokenKind::Comma | TokenKind::RBrace => {}
            _ => {
                return Err(nom::Err::Failure(ParseErr::expected(
                    "改行または `}` (アームの終わり)",
                    input.first(),
                )));
            }
        }
    }
    let (input, rbrace) = expect(input, &TokenKind::RBrace, "`}`")?;
    let span = start.merge(rbrace.span);
    Ok((
        input,
        Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            span,
        },
    ))
}

/// パターンを 1 つ解析する。
fn parse_pattern(input: Tokens) -> PResult<Pattern> {
    let tok = input.first();
    let span = tok.span;
    match &tok.kind {
        // ワイルドカード `_`
        TokenKind::Ident(name) if name == "_" => {
            Ok((input.take_from(1), Pattern::Wildcard { span }))
        }
        // 整数リテラル（負数は `-` を前置した単項として扱わない——パターンは単純な値）。
        // `1..10` / `1..=10` の範囲パターンにもなる。
        TokenKind::Int(v) => parse_range_or_lit(input.take_from(1), LitPat::Int(*v), span),
        TokenKind::Float(v) => parse_range_or_lit(input.take_from(1), LitPat::Float(*v), span),
        TokenKind::Bool(b) => {
            let b = *b;
            Ok((
                input.take_from(1),
                Pattern::Lit { value: LitPat::Bool(b), span },
            ))
        }
        TokenKind::Str(s) => {
            let s = s.clone();
            Ok((
                input.take_from(1),
                Pattern::Lit { value: LitPat::Str(s), span },
            ))
        }
        // `Name(binding)` または素の識別子パターン。
        TokenKind::Ident(name) => {
            let name = name.clone();
            let rest = input.take_from(1);
            // `Name(binding)` — ペイロード束縛付きバリアント。
            if rest.peek() == &TokenKind::LParen {
                let (rest, _) = eat(rest, &TokenKind::LParen, "`(`")?;
                let (rest, (bname, bspan)) = ident(rest)?;
                let (rest, rp) = expect(rest, &TokenKind::RParen, "`)`")?;
                let full_span = span.merge(rp.span);
                Ok((
                    rest,
                    Pattern::Variant {
                        name,
                        binding: (bname, bspan),
                        span: full_span,
                    },
                ))
            } else {
                // 素の識別子 — バリアント名か束縛変数かは typeck で判定。
                Ok((rest, Pattern::Bind { name, span }))
            }
        }
        _ => Err(nom::Err::Error(ParseErr::expected(
            "パターン (`_`・リテラル・バリアント名)",
            tok,
        ))),
    }
}

/// 数値リテラル `lo` を読んだ直後の位置から、範囲パターン `lo..hi` / `lo..=hi`
/// か単独リテラルパターンかを判定して解析する。`input` は `lo` の次のトークンを指す。
fn parse_range_or_lit(input: Tokens, lo: LitPat, lo_span: Span) -> PResult<Pattern> {
    let inclusive = match input.peek() {
        TokenKind::DotDot => false,
        TokenKind::DotDotEq => true,
        // `..` でなければ単独リテラルパターン。
        _ => return Ok((input, Pattern::Lit { value: lo, span: lo_span })),
    };
    let rest = input.take_from(1);
    // 上限は整数・浮動小数リテラルのみ。
    let hi_tok = rest.first();
    let hi = match &hi_tok.kind {
        TokenKind::Int(v) => LitPat::Int(*v),
        TokenKind::Float(v) => LitPat::Float(*v),
        _ => {
            return Err(nom::Err::Failure(ParseErr::expected(
                "範囲の上限 (整数・浮動小数リテラル)",
                hi_tok,
            )));
        }
    };
    let full_span = lo_span.merge(hi_tok.span);
    Ok((
        rest.take_from(1),
        Pattern::Range { lo, hi, inclusive, span: full_span },
    ))
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
