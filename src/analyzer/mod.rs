pub mod errors;
pub mod symbol;
#[cfg(test)]
mod tests;
pub mod type_checker;

pub use errors::TypeError;
pub use symbol::{OwnershipState, Symbol, SymbolTable};
pub use type_checker::TypeChecker;
