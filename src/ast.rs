/// スパン情報付きノード
pub type Span = (usize, usize);

/// トップレベル宣言
#[derive(Debug, Clone)]
pub enum Item {
    LetStmt(LetStmt),
    FuncDef(FuncDef),
    UseStmt(UseStmt),
    TraitDef(TraitDef),
    ImplDef(ImplDef),
    ComptimeDirective(ComptimeDirective),
}

/// `let name: Type = expr`
#[derive(Debug, Clone)]
pub struct LetStmt {
    pub name: String,
    pub ty: Option<Type>,
    pub value: Expr,
    pub span: Span,
}

/// `fn name<Generics>(params): RetType -> { stmts }` or `-> pipeline`
#[derive(Debug, Clone)]
pub struct FuncDef {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub ret_ty: Type,
    pub body: FuncBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum FuncBody {
    Block(Vec<Stmt>),
    Pipeline(Pipeline),
}

/// 関数パラメータ
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

/// `use path.to.module.*`
#[derive(Debug, Clone)]
pub struct UseStmt {
    pub path: Vec<PathSegment>,
    pub glob: bool, // `.*` があるか
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum PathSegment {
    Ident(String),
    Trait(String), // #Foo
}

/// `trait #Name : #Parent { members }`
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    pub parent: Option<String>,
    pub members: Vec<TraitMember>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TraitMember {
    MethodSig(MethodSig),
    FuncDef(FuncDef),
}

/// `fn name(params): Type`  (実装なし)
#[derive(Debug, Clone)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub ret_ty: Type,
    pub span: Span,
}

/// `impl #Trait for Type { members }`
#[derive(Debug, Clone)]
pub struct ImplDef {
    pub trait_name: String,
    pub for_ty: Type,
    pub members: Vec<FuncDef>,
    pub span: Span,
}

/// `!name(args)`
#[derive(Debug, Clone)]
pub struct ComptimeDirective {
    pub name: String,
    pub args: Vec<Expr>,
    pub span: Span,
}

/// ジェネリクスパラメータ `T: #Bound && #Bound2`
#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<String>, // Trait名リスト
}

/// 型
#[derive(Debug, Clone)]
pub enum Type {
    Named(String),
    Generic(String, Vec<Type>),
    Trait(String), // #Foo
}

/// 文
#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    Expr(Expr),
    Item(Item),
}

/// 式
#[derive(Debug, Clone)]
pub enum Expr {
    // リテラル
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    // 識別子
    Ident(String),
    // 関数呼び出し `f(a, b)`
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    // 二項演算
    BinOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    // パイプライン式
    Pipeline(Box<Pipeline>),
    // ラムダ `(x, y -> expr)`
    Lambda {
        params: Vec<LambdaParam>,
        body: Box<Expr>,
    },
    // 首なしパイプライン `( :: trim() :: lower() )`
    HeadlessPipeline(Vec<PipelineStep>),
}

/// パイプライン: `subject :: step :: step ...`
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub subject: Box<Expr>,
    pub steps: Vec<(BackboneOp, PipelineStep)>,
    pub span: Span,
}

/// 背骨演算子
#[derive(Debug, Clone, PartialEq)]
pub enum BackboneOp {
    Next,   // ::
    Await,  // :~
    Try,    // :^
    Force,  // :!
    Catch,  // :?
    Or,     // :|
    Tag,    // :>
    Join,   // :&
}

/// パイプラインのステップ
#[derive(Debug, Clone)]
pub enum PipelineStep {
    /// `trim()`  主語が第1引数に自動バインド
    ImplicitCall {
        func: String,
        args: Vec<Expr>,
    },
    /// `includes(base, $)` $が主語
    ExplicitCall {
        func: String,
        args: Vec<PlaceholderArg>,
    },
    /// `foo$x(x + bar(x))`
    AliasedCall {
        func: String,
        alias: String,
        body: Box<Expr>,
    },
    /// `* factor`
    BinOpRhs {
        op: BinOp,
        rhs: Box<Expr>,
    },
    /// FlowStructure (match, join, if, while)
    Flow(FlowStructure),
}

#[derive(Debug, Clone)]
pub enum PlaceholderArg {
    Expr(Expr),
    Dollar, // $
}

/// 制御フロー構造
#[derive(Debug, Clone)]
pub enum FlowStructure {
    /// `match ( pattern -> pipeline, ... )`
    Match(Vec<MatchBranch>),
    /// `( | :: pipeline | expr :: pipeline )`
    Join(Vec<JoinBranch>),
    /// `:if cond :then expr :else expr`
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Option<Box<Expr>>,
    },
    /// `:while cond :then expr`
    While {
        cond: Box<Expr>,
        then: Box<Expr>,
    },
}

#[derive(Debug, Clone)]
pub struct MatchBranch {
    pub pattern: Pattern,
    pub body: Pipeline,
}

#[derive(Debug, Clone)]
pub struct JoinBranch {
    pub inherit_subject: bool, // `| ::` で始まるか
    pub pipeline: Pipeline,
}

/// パターン（match用）
#[derive(Debug, Clone)]
pub enum Pattern {
    Ident(String),
    Constructor(String, Vec<Pattern>),
    Wildcard,
    Literal(Expr),
}

/// 二項演算子
#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// ラムダパラメータ
#[derive(Debug, Clone)]
pub struct LambdaParam {
    pub name: String,
    pub ty: Option<Type>,
}
