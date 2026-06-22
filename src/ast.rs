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
    /// トレイト定義 `trait Name<T>: Super { fn ... }`（ADR-0007）。
    Trait(TraitDef),
    /// 実装ブロック `impl [Trait for] Type { fn ... }`。
    /// `trait_ref` が `None` なら固有メソッド、`Some` なら `impl Trait for Type`。
    Impl(Impl),
}

/// トレイト定義 `trait Name<T>: Super1 + Super2 { メソッド... }`。
/// メソッドはシグネチャのみ、または既定実装（本体付き）。
#[derive(Debug, Clone, PartialEq)]
pub struct TraitDef {
    pub is_pub: bool,
    pub name: String,
    pub name_span: Span,
    /// トレイトの型パラメータ `trait Iterator<T>`（ADR-0007）。
    pub generics: Vec<Generic>,
    /// スーパートレイト `trait Sub: Super`（ADR-0007）。要求集合。
    pub supertraits: Vec<TraitRef>,
    pub methods: Vec<TraitMethod>,
    pub span: Span,
}

/// トレイトのメソッド。`default` が true のとき `func.body` が既定実装。
/// false のときシグネチャのみ（`func.body` は空ブロック）。
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub func: Function,
    pub default: bool,
}

/// トレイト参照 `Name<Args>`。`impl Trait for`・スーパートレイト・境界 `<T: Bound>`・
/// 呼び出し修飾子 `#Trait<Args>` で共通に使う（ADR-0004 / 0007）。
#[derive(Debug, Clone, PartialEq)]
pub struct TraitRef {
    pub name: String,
    pub args: Vec<Type>,
    pub span: Span,
}

/// `impl [Trait for] Type { メソッド... }`。型へメソッドをまとめる。
#[derive(Debug, Clone, PartialEq)]
pub struct Impl {
    /// `impl Trait for Type` の `Trait<Args>`。固有メソッドの `impl Type` では `None`。
    pub trait_ref: Option<TraitRef>,
    /// 実装対象の型名（`impl Point` の `Point`）。
    pub type_name: String,
    pub type_name_span: Span,
    /// メソッド群。第一引数の `self`（[`SelfKind`]）は `params` の先頭に合成される。
    pub methods: Vec<Function>,
    pub span: Span,
}

/// メソッドの `self` の受け方。`None`（self なし）は関連関数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    /// `self`（値で受ける＝ムーブ）
    Value,
    /// `&self`（不変参照で借用）
    Ref,
    /// `&mut self`（可変参照で借用）
    RefMut,
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

/// 型パラメータ `<T>` / `<T: Bound1 + Bound2>`。
#[derive(Debug, Clone, PartialEq)]
pub struct Generic {
    pub name: String,
    /// トレイト境界 `<T: Greet + Serialize>`（ADR-0006）。空なら無制約。
    pub bounds: Vec<TraitRef>,
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
    /// 関数の型パラメータ `fn f<T: Bound>(...)`（ADR-0006・単相化）。
    pub generics: Vec<Generic>,
    /// メソッドの `self` の受け方。`self` を取るとき `params` の先頭が合成 self。
    /// 自由関数・self なしの関連関数では `None`。
    pub self_kind: Option<SelfKind>,
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
    /// イテレータループ `for x in start..end { ... }`（現状は整数範囲のみ）。
    /// `inclusive` が真なら `..=`（上限を含む）、偽なら `..`（上限を含まない）。
    For {
        var: String,
        var_span: Span,
        start: Expr,
        end: Expr,
        inclusive: bool,
        body: Block,
        span: Span,
    },
    /// 一般イテレータループ `for x in iter { ... }`（ADR-0007）。`iter` は `Iterator<T>`
    /// を実装する値で、要素型 `T` が `var` に束縛される。範囲 `for` とは別ノード。
    ForIn {
        var: String,
        var_span: Span,
        iter: Expr,
        body: Block,
        span: Span,
    },
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
    /// メンバアクセス `object.field`、またはメソッド呼び出しの被メンバ。
    /// `qualifier` はメソッド名衝突の修飾子 `object.field#Trait<Args>`（ADR-0004）。
    /// フィールドアクセスでは常に `None`。
    Member {
        object: Box<Expr>,
        field: String,
        qualifier: Option<TraitRef>,
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
    /// enum バリアント構築 `VariantName` / `VariantName(payload)`。
    /// パーサは `Call` / `Ident` として生成し、typeck が認識して型を付ける。
    /// コード生成ではこのノードとして扱う（typeck が TypeInfo へ記録する）。
    EnumLit {
        variant: String,
        payload: Option<Box<Expr>>,
        span: Span,
    },
    /// match 式 `match scrutinee { pattern -> expr ... }`。
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// 配列リテラル `[e1, e2, ...]`。型注釈に応じて固定長配列 `T[]` または
    /// 動的配列 `Vec<T>` を構築する（型指向。typeck が解決して型を記録する）。
    ArrayLit { elems: Vec<Expr> },
}

/// match 式のアーム `pattern [if guard] -> body`。
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    /// ガード条件 `if cond`。なければ `None`。bool 型で、パターンの束縛変数を参照できる。
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

/// match のパターン。
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `_`
    Wildcard { span: Span },
    /// リテラル `0`, `true`, `"str"`, `3.14`
    Lit { value: LitPat, span: Span },
    /// 数値の範囲 `1..10`（排他）/ `1..=10`（包含）。境界は整数または浮動小数リテラル。
    Range {
        lo: LitPat,
        hi: LitPat,
        /// `..=`（上限を含む）なら true、`..`（上限を含まない）なら false。
        inclusive: bool,
        span: Span,
    },
    /// `VariantName` または `VariantName(binding)`
    Variant {
        name: String,
        /// ペイロードを束縛する変数名と宣言 span。ペイロードなしのバリアントでは `None`。
        binding: Option<(String, Span)>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wildcard { span }
            | Pattern::Lit { span, .. }
            | Pattern::Range { span, .. }
            | Pattern::Variant { span, .. } => *span,
        }
    }
}

/// リテラルパターンの値。
#[derive(Debug, Clone, PartialEq)]
pub enum LitPat {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
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
    /// 引数位置の匿名トレイト境界 `x: Greet + Serialize`（ADR-0006）。
    /// parse_function が匿名ジェネリックパラメータへ脱糖するため、通常は引数型としてのみ現れる。
    Bound { bounds: Vec<TraitRef>, span: Span },
}

impl Type {
    pub fn span(&self) -> Span {
        match self {
            Type::Named { span, .. }
            | Type::Ref { span, .. }
            | Type::Array { span, .. }
            | Type::Tuple { span, .. }
            | Type::Bound { span, .. } => *span,
        }
    }
}
