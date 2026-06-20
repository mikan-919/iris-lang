//! 抽象構文木。縦切りの範囲（関数定義 / let・const / return / 式 / 型）を表す。
//! 各ノードは診断のため span を保持する。

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(Function),
    TypeDef(TypeDef),
}

/// 型定義 `type Name<generics> = (別名 | struct | enum)`。
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    pub is_pub: bool,
    pub name: String,
    pub name_span: Span,
    pub generics: Vec<Generic>,
    pub body: TypeDefBody,
    pub span: Span,
}

/// 型パラメータ `<T>`（トレイト境界は未対応）。
#[derive(Debug, Clone, PartialEq)]
pub struct Generic {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeDefBody {
    /// `type Name = T`（名前的型付けにより別の型）
    Alias(Type),
    /// `type Name = struct { field... }`
    Struct(Vec<Field>),
    /// `type Name = enum { Variant... }`
    Enum(Vec<Variant>),
}

/// struct のフィールド `name: Type`。
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

/// enum のバリアント `Name` または `Name: Type`（ペイロード付き）。
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub payload: Option<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub is_pub: bool,
    /// `extern fn`（本体なし・C の関数を宣言）。`body` は空ブロック。
    pub is_extern: bool,
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let [mut] name [: Type] = value` / `const name [: Type] = value`
    Let {
        is_const: bool,
        mutable: bool,
        name: String,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    /// `return [value]`
    Return { value: Option<Expr>, span: Span },
    /// 再代入 `target = value`（宣言側の `mut` が前提）
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    /// 条件ループ `while cond { ... }`
    While {
        cond: Expr,
        body: Block,
        span: Span,
    },
    /// 無限ループ `loop { ... }`（`break` で抜ける）
    Loop { body: Block, span: Span },
    /// `break`（最も内側のループを抜ける）
    Break { span: Span },
    /// `continue`（最も内側のループの先頭へ）
    Continue { span: Span },
    /// 式文
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Ident(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// 関数呼び出し `callee(args...)`
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// メンバアクセス `object.field`
    Member {
        object: Box<Expr>,
        field: String,
    },
    /// 三項演算子 `cond ? then : else`
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// エラー伝播の後置 `!` 演算子 `expr!`
    Try(Box<Expr>),
    /// 式としての if
    If {
        cond: Box<Expr>,
        then: Block,
        otherwise: Option<Box<Else>>,
    },
    /// 構造体リテラル `Name { field: value, ... }`
    StructLit {
        name: String,
        name_span: Span,
        fields: Vec<FieldInit>,
    },
}

/// 構造体リテラルのフィールド初期化 `name: value`。
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Else {
    /// `else if ...`
    If(Expr),
    /// `else { ... }`
    Block(Block),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-x`
    Neg,
    /// `&x`
    Ref,
    /// `&mut x`
    RefMut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// 名前付き型。`Foo`, `Vec<i32>`, `i32`
    Named {
        name: String,
        args: Vec<Type>,
        span: Span,
    },
    /// 参照型。`&T`, `&mut T`
    Ref {
        mutable: bool,
        inner: Box<Type>,
        span: Span,
    },
    /// 固定長配列型。`T[]`
    Array { inner: Box<Type>, span: Span },
    /// タプル型。`(A, B)`
    Tuple { elems: Vec<Type>, span: Span },
}

impl Type {
    pub fn span(&self) -> Span {
        match self {
            Type::Named { span, .. }
            | Type::Ref { span, .. }
            | Type::Array { span, .. }
            | Type::Tuple { span, .. } => *span,
        }
    }
}
