//! 型検査で用いる内部型表現 [`Ty`]。
//!
//! AST の [`crate::ast::Type`] を解決・正規化したもの。リテラルの未確定型
//! （[`Ty::IntLit`] / [`Ty::FloatLit`]）と、ジェネリクスの穴やメンバ等の
//! 不明型 [`Ty::Infer`]、エラー伝播を止める番兵 [`Ty::Error`] を持つ。

use crate::ast::Type;

/// 符号付き・符号なし整数のプリミティブ型名。
pub const INT_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64",
];
/// 浮動小数点のプリミティブ型名。
pub const FLOAT_TYPES: &[&str] = &["f32", "f64"];

/// 内部型表現。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// 名前付き型。プリミティブ（`i32`, `bool`, `string`, `void` …）や
    /// ジェネリクス（`Vec<i32>`, `Result<T, E>`）を表す。
    Named { name: String, args: Vec<Ty> },
    /// 参照型 `&T` / `&mut T`。
    Ref { mutable: bool, inner: Box<Ty> },
    /// 配列型 `T[]`。
    Array(Box<Ty>),
    /// タプル型 `(A, B)`。
    Tuple(Vec<Ty>),
    /// 型が未確定の整数リテラル。任意の整数型に適合する。
    IntLit,
    /// 型が未確定の浮動小数リテラル。任意の浮動小数型に適合する。
    FloatLit,
    /// 不明な型（ジェネリクスの穴・メンバアクセス等）。何にでも適合する。
    Infer,
    /// 型エラー。これ以上エラーを波及させないための番兵。
    Error,
}

impl Ty {
    /// 値を持たない型 `void`。
    pub fn unit() -> Ty {
        Ty::named("void")
    }

    /// 引数なしの名前付き型を作る。
    pub fn named(name: &str) -> Ty {
        Ty::Named {
            name: name.to_string(),
            args: Vec::new(),
        }
    }

    /// AST の型注釈から内部型へ変換する。
    pub fn from_ast(t: &Type) -> Ty {
        match t {
            Type::Named { name, args, .. } => Ty::Named {
                name: name.clone(),
                args: args.iter().map(Ty::from_ast).collect(),
            },
            Type::Ref { mutable, inner, .. } => Ty::Ref {
                mutable: *mutable,
                inner: Box::new(Ty::from_ast(inner)),
            },
            Type::Array { inner, .. } => Ty::Array(Box::new(Ty::from_ast(inner))),
            Type::Tuple { elems, .. } => Ty::Tuple(elems.iter().map(Ty::from_ast).collect()),
        }
    }

    /// 整数型（具体名 or リテラル）か。
    pub fn is_integer(&self) -> bool {
        match self {
            Ty::IntLit => true,
            Ty::Named { name, args } => args.is_empty() && INT_TYPES.contains(&name.as_str()),
            _ => false,
        }
    }

    /// 浮動小数型（具体名 or リテラル）か。
    pub fn is_float(&self) -> bool {
        match self {
            Ty::FloatLit => true,
            Ty::Named { name, args } => args.is_empty() && FLOAT_TYPES.contains(&name.as_str()),
            _ => false,
        }
    }

    /// 数値型か。
    pub fn is_numeric(&self) -> bool {
        self.is_integer() || self.is_float()
    }

    /// `bool` 型か。
    pub fn is_bool(&self) -> bool {
        matches!(self, Ty::Named { name, args } if args.is_empty() && name == "bool")
    }

    /// 診断表示用の文字列。
    pub fn describe(&self) -> String {
        match self {
            Ty::Named { name, args } if args.is_empty() => name.clone(),
            Ty::Named { name, args } => {
                let inner: Vec<_> = args.iter().map(Ty::describe).collect();
                format!("{name}<{}>", inner.join(", "))
            }
            Ty::Ref { mutable: true, inner } => format!("&mut {}", inner.describe()),
            Ty::Ref { mutable: false, inner } => format!("&{}", inner.describe()),
            Ty::Array(inner) => format!("{}[]", inner.describe()),
            Ty::Tuple(elems) => {
                let inner: Vec<_> = elems.iter().map(Ty::describe).collect();
                format!("({})", inner.join(", "))
            }
            Ty::IntLit => "整数リテラル".to_string(),
            Ty::FloatLit => "小数リテラル".to_string(),
            Ty::Infer => "_".to_string(),
            Ty::Error => "<error>".to_string(),
        }
    }

    /// リテラルの未確定型を既定の具体型へ確定する（`IntLit`→`i32`, `FloatLit`→`f64`）。
    /// それ以外はそのまま返す。
    pub fn defaulted(self) -> Ty {
        match self {
            Ty::IntLit => Ty::named("i32"),
            Ty::FloatLit => Ty::named("f64"),
            other => other,
        }
    }
}
