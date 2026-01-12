use crate::ast::Type;
use std::fmt;

#[derive(Debug)]
pub enum TypeError {
    UndefinedVariable(String),
    UndefinedFunction(String),
    TypeMismatch {
        expected: Type,
        got: Type,
        location: String,
    },
    OwnershipViolation {
        var_name: String,
        reason: String,
    },
    ArityMismatch {
        func_name: String,
        expected: usize,
        got: usize,
    },
    InvalidUnwrap {
        ty: Type,
    },
    InvalidFallback {
        ty: Type,
    },
    InvalidErrorRescue {
        ty: Type,
    },
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::UndefinedVariable(name) => {
                write!(f, "Type error: Undefined variable '{}'", name)
            }
            TypeError::UndefinedFunction(name) => {
                write!(f, "Type error: Undefined function '{}'", name)
            }
            TypeError::TypeMismatch {
                expected,
                got,
                location,
            } => {
                write!(
                    f,
                    "Type error at {}: expected {}, got {}",
                    location, expected, got
                )
            }
            TypeError::OwnershipViolation { var_name, reason } => {
                write!(f, "Ownership error for '{}': {}", var_name, reason)
            }
            TypeError::ArityMismatch {
                func_name,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Arity error for '{}': expected {} arguments, got {}",
                    func_name, expected, got
                )
            }
            TypeError::InvalidUnwrap { ty } => {
                write!(f, "Cannot force unwrap non-Option/Result type {}", ty)
            }
            TypeError::InvalidFallback { ty } => {
                write!(f, "Cannot use fallback on non-Option type {}", ty)
            }
            TypeError::InvalidErrorRescue { ty } => {
                write!(f, "Cannot use error rescue on non-Result type {}", ty)
            }
        }
    }
}
