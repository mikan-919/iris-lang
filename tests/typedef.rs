//! 型定義（type/struct/enum）と構造体リテラルの回帰テスト。

use iris_lang::ast::{Item, TypeDefBody};
use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::sema::{check, resolve};

fn program(src: &str) -> iris_lang::ast::Program {
    let tokens = lex(src).expect("字句解析");
    parse(&tokens).expect("構文解析")
}

/// 型検査まで通す。エラーがあればメッセージ一覧を返す。
fn typecheck(src: &str) -> Result<(), Vec<String>> {
    let prog = program(src);
    let resolution = resolve(&prog).expect("名前解決");
    check(&prog, &resolution)
        .map(|_| ())
        .map_err(|errs| errs.into_iter().map(|e| e.message).collect())
}

// ---- パース ------------------------------------------------------------

#[test]
fn parses_struct_alias_enum() {
    let prog = program(
        "type Meters = f64\ntype Rect = struct {\n    w: f64\n    h: f64\n}\ntype Color = enum { Red, Green, Blue }",
    );
    assert_eq!(prog.items.len(), 3);
    let Item::TypeDef(alias) = &prog.items[0] else {
        panic!("型定義のはず");
    };
    assert!(matches!(alias.body, TypeDefBody::Alias(_)));
    let Item::TypeDef(rect) = &prog.items[1] else {
        panic!("型定義のはず");
    };
    let TypeDefBody::Struct(fields) = &rect.body else {
        panic!("struct のはず");
    };
    assert_eq!(fields.len(), 2);
    let Item::TypeDef(color) = &prog.items[2] else {
        panic!("型定義のはず");
    };
    let TypeDefBody::Enum(variants) = &color.body else {
        panic!("enum のはず");
    };
    assert_eq!(variants.len(), 3);
}

#[test]
fn parses_generic_struct() {
    let prog = program("type Box2<T> = struct {\n    value: T\n}");
    let Item::TypeDef(t) = &prog.items[0] else {
        panic!("型定義のはず");
    };
    assert_eq!(t.generics.len(), 1);
    assert_eq!(t.generics[0].name, "T");
}

#[test]
fn struct_literal_does_not_break_if() {
    // `if cond { ... }` の条件位置では構造体リテラルとして誤解析しない。
    typecheck("fn f(c: bool): i32 {\n    if c {\n        return 1\n    }\n    return 0\n}")
        .expect("if が壊れないこと");
}

// ---- 型検査 ------------------------------------------------------------

#[test]
fn member_access_yields_field_type() {
    let src = "type Rect = struct {\n    w: f64\n    h: f64\n}\nfn area(r: Rect): f64 {\n    return r.w * r.h\n}";
    typecheck(src).expect("メンバアクセスの型が通る");
}

#[test]
fn rejects_unknown_field_access() {
    let src = "type Rect = struct {\n    w: f64\n}\nfn f(r: Rect): f64 {\n    return r.height\n}";
    let errs = typecheck(src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("フィールド `height` はありません")));
}

#[test]
fn struct_literal_typechecks() {
    let src = "type Rect = struct {\n    w: f64\n    h: f64\n}\nfn make(): Rect {\n    return Rect { w: 1.0, h: 2.0 }\n}";
    typecheck(src).expect("構造体リテラルが通る");
}

#[test]
fn rejects_struct_literal_field_type_mismatch() {
    let src = "type Rect = struct {\n    w: f64\n}\nfn make(): Rect {\n    return Rect { w: true }\n}";
    let errs = typecheck(src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("フィールド `w` の型が一致しません")));
}

#[test]
fn rejects_missing_and_unknown_fields() {
    let missing = typecheck(
        "type Rect = struct {\n    w: f64\n    h: f64\n}\nfn make(): Rect {\n    return Rect { w: 1.0 }\n}",
    )
    .unwrap_err();
    assert!(missing.iter().any(|m| m.contains("`h` が初期化されていません")));

    let unknown = typecheck(
        "type Rect = struct {\n    w: f64\n}\nfn make(): Rect {\n    return Rect { w: 1.0, z: 2.0 }\n}",
    )
    .unwrap_err();
    assert!(unknown.iter().any(|m| m.contains("フィールド `z` はありません")));
}

#[test]
fn rejects_unknown_type_name() {
    let errs = typecheck("fn f(x: Nope): i32 {\n    return 0\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("未定義の型 `Nope`")));
}

#[test]
fn alias_is_nominal_but_accepts_literal() {
    // リテラルは別名の元の型へ適合する。
    typecheck("type Meters = f64\nfn f() {\n    let m: Meters = 5.0\n}").expect("リテラルは適合");

    // しかし f64 値を Meters へは渡せない（名前的型付け）。
    let errs = typecheck(
        "type Meters = f64\nfn use_m(m: Meters) {\n}\nfn f(x: f64) {\n    use_m(x)\n}",
    )
    .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("引数の型が一致しません")));
}

#[test]
fn struct_value_constructed_and_passed() {
    let src = "type P = struct {\n    x: i32\n    y: i32\n}\nfn sum(p: P): i32 {\n    return p.x + p.y\n}\nfn run(): i32 {\n    return sum(P { x: 1, y: 2 })\n}";
    typecheck(src).expect("構築して関数へ渡せる");
}
