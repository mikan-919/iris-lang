use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Sub => write!(f, "-"),
            BinaryOp::Mul => write!(f, "*"),
            BinaryOp::Div => write!(f, "/"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Unit,
    Any,
    Simple(String),
    Tuple(Vec<Type>),
    Generic {
        name: String,
        args: Vec<Type>,
    },
    TypeVar(usize),
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
}

impl Type {
    pub fn option(inner: Type) -> Self {
        Type::Generic {
            name: "Option".to_string(),
            args: vec![inner],
        }
    }

    pub fn result(ok: Type, err: Type) -> Self {
        Type::Generic {
            name: "Result".to_string(),
            args: vec![ok, err],
        }
    }

    pub fn future(inner: Type) -> Self {
        Type::Generic {
            name: "Future".to_string(),
            args: vec![inner],
        }
    }

    pub fn normalize(&self) -> Self {
        match self {
            Type::Simple(name) => match name.as_str() {
                "Int" => Type::Int,
                "Float" => Type::Float,
                "Bool" => Type::Bool,
                "String" => Type::String,
                "Unit" => Type::Unit,
                "Any" => Type::Any,
                _ => Type::Simple(name.clone()),
            },
            Type::Tuple(elems) => Type::Tuple(elems.iter().map(|t| t.normalize()).collect()),
            Type::Generic { name, args } => Type::Generic {
                name: name.clone(),
                args: args.iter().map(|t| t.normalize()).collect(),
            },
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(|t| t.normalize()).collect(),
                return_type: Box::new(return_type.normalize()),
            },
            _ => self.clone(),
        }
    }

    pub fn to_wasm_type(&self) -> Result<&'static str, String> {
        match self {
            Type::Int => Ok("i32"),
            Type::Float => Ok("f64"),
            Type::Bool => Ok("i32"),
            Type::Unit => Ok("i32"),
            _ => Err(format!("Type {} not yet supported in Wasm", self)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Identifier(String),
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Pipeline {
        initial: Box<Expr>,
        steps: Vec<PipelineStep>,
    },
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Block(Vec<Stmt>),
    Join(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PipelineStep {
    FunctionCall { name: String, args: Vec<Expr> },
    AsyncCall(String),
    ErrorPropagate,
    Force,
    ErrorRescue(Box<Expr>),
    Fallback(Box<Expr>),
    BorrowReference(String),
    TupleMerge(Box<Expr>),
    MatchArm(Box<MatchArm>),
    ForLoop(Box<ForLoop>),
    ArithmeticBinaryOp { op: BinaryOp, right: Box<Expr> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Identifier(String),
    Constructor { name: String, args: Vec<Pattern> },
    Literal(Literal),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchArm {
    Arm { pattern: Pattern, expr: Expr },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionBody {
    Expression(Box<Expr>),
    External(String),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForLoop {
    pub subject: Box<Expr>,
    pub item: String,
    pub collection: Box<Expr>,
    pub body: Box<Expr>,
    pub is_threading: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Binding {
        name: String,
        expr: Expr,
    },
    FunctionDefinition {
        name: String,
        is_exported: bool,
        params: Vec<(String, Type)>,
        return_type: Type,
        body: FunctionBody,
    },
    ImportStatement {
        names: Vec<String>,
        module: String,
    },
    Block(Vec<Stmt>),
    MatchStatement(Box<Expr>),
    ForLoopStatement(Box<ForLoop>),
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Float => write!(f, "Float"),
            Type::Bool => write!(f, "Bool"),
            Type::String => write!(f, "String"),
            Type::Unit => write!(f, "()"),
            Type::Any => write!(f, "Any"),
            Type::Simple(name) => write!(f, "{}", name),
            Type::Tuple(elems) => {
                write!(f, "(")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", elem)?;
                }
                write!(f, ")")
            }
            Type::Generic { name, args } => {
                write!(f, "{}<", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ">")
            }
            Type::TypeVar(id) => write!(f, "'t{}", id),
            Type::Function {
                params,
                return_type,
            } => {
                write!(f, "(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", return_type)
            }
        }
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Integer(n) => write!(f, "{}", n),
            Literal::String(s) => write!(f, "\"{}\"", s),
        }
    }
}

impl fmt::Display for PipelineStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineStep::FunctionCall { name, args } => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            PipelineStep::AsyncCall(name) => write!(f, ":~ {}()", name),
            PipelineStep::ErrorPropagate => write!(f, ":^"),
            PipelineStep::Force => write!(f, ":!"),
            PipelineStep::ErrorRescue(expr) => write!(f, ":? {}", expr),
            PipelineStep::Fallback(expr) => write!(f, ":| {}", expr),
            PipelineStep::BorrowReference(name) => write!(f, ":> {}", name),
            PipelineStep::TupleMerge(expr) => write!(f, ":& {}", expr),
            PipelineStep::MatchArm(arm) => write!(f, "| {} ::", arm),
            PipelineStep::ForLoop(for_loop) => write!(
                f,
                " :: for {}@{} :: {}",
                for_loop.item, for_loop.collection, for_loop.body
            ),
            PipelineStep::ArithmeticBinaryOp { op, right } => {
                let op_str = match op {
                    BinaryOp::Add => "+",
                    BinaryOp::Sub => "-",
                    BinaryOp::Mul => "*",
                    BinaryOp::Div => "/",
                };
                write!(f, " :: {}{}", op_str, right)
            }
        }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pattern::Identifier(name) => write!(f, "{}", name),
            Pattern::Constructor { name, args } => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Pattern::Literal(lit) => write!(f, "{}", lit),
        }
    }
}

impl fmt::Display for MatchArm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MatchArm::Arm { pattern, expr } => write!(f, "{} =: {}", pattern, expr),
        }
    }
}

impl fmt::Display for ForLoop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{} :: {}", self.item, self.collection, self.body)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Literal(lit) => write!(f, "{}", lit),
            Expr::Identifier(name) => write!(f, "{}", name),
            Expr::FunctionCall { name, args } => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expr::BinaryOp { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
            Expr::Pipeline { initial, steps } => {
                write!(f, "{}", initial)?;
                for step in steps {
                    write!(f, " {}", step)?;
                }
                Ok(())
            }
            Expr::Match { subject, arms } => {
                write!(f, "match {} ", subject)?;
                for (i, arm) in arms.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", arm)?;
                }
                Ok(())
            }
            Expr::Block(stmts) => {
                write!(f, "{{")?;
                for stmt in stmts {
                    write!(f, " {}", stmt)?;
                }
                write!(f, " }}")
            }
            Expr::Join(exprs) => {
                write!(f, "(")?;
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", expr)?;
                }
                write!(f, ")")
            }
        }
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Binding { name, expr } => write!(f, "let {} =: {}", name, expr),
            Stmt::FunctionDefinition {
                name,
                is_exported,
                params,
                return_type,
                body,
            } => {
                if *is_exported {
                    write!(f, "export ")?;
                }
                write!(f, "fn {}(", name)?;
                for (i, (param_name, param_type)) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", param_name, param_type)?;
                }
                write!(f, ") -> {} ", return_type)?;
                match body {
                    FunctionBody::Expression(expr) => write!(f, "= {}", expr),
                    FunctionBody::External(external) => write!(f, "=: \"{}\"", external),
                    FunctionBody::Block(stmts) => {
                        write!(f, "{{")?;
                        for stmt in stmts {
                            write!(f, " {}", stmt)?;
                        }
                        write!(f, " }}")
                    }
                }
            }
            Stmt::ImportStatement { names, module } => {
                write!(f, "import {{ ")?;
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", name)?;
                }
                write!(f, " }} from \"{}\"", module)
            }
            Stmt::Block(stmts) => {
                write!(f, "{{")?;
                for stmt in stmts {
                    write!(f, " {}", stmt)?;
                }
                write!(f, " }}")
            }
            Stmt::MatchStatement(expr) => write!(f, "match {}", expr),
            Stmt::ForLoopStatement(for_loop) => write!(f, "for {}", for_loop),
        }
    }
}
