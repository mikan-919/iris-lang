use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Simple(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Identifier(String),
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
    Pipeline {
        initial: Box<Expr>,
        steps: Vec<PipelineStep>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PipelineStep {
    FunctionCall(String),
    AsyncCall(String),
    ErrorPropagate,
    Force,
    ErrorRescue(Box<Expr>),
    Fallback(Box<Expr>),
    BorrowReference(String),
    TupleMerge(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Binding {
        name: String,
        expr: Expr,
    },
    FunctionDefinition {
        name: String,
        params: Vec<(String, Type)>,
        return_type: Type,
        body: Box<Expr>,
    },
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Simple(name) => write!(f, "{}", name),
        }
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
            Expr::Pipeline { initial, steps } => {
                write!(f, "{}", initial)?;
                for step in steps {
                    write!(f, " {}", step)?;
                }
                Ok(())
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
            PipelineStep::FunctionCall(name) => write!(f, "{}()", name),
            PipelineStep::AsyncCall(name) => write!(f, ":~ {}()", name),
            PipelineStep::ErrorPropagate => write!(f, ":^"),
            PipelineStep::Force => write!(f, ":!"),
            PipelineStep::ErrorRescue(expr) => write!(f, ":? {}", expr),
            PipelineStep::Fallback(expr) => write!(f, ":| {}", expr),
            PipelineStep::BorrowReference(name) => write!(f, ":> {}", name),
            PipelineStep::TupleMerge(expr) => write!(f, ":& {}", expr),
        }
    }
}
