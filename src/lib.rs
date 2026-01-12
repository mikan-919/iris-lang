pub mod analyzer;
pub mod ast;
pub mod codegen;
pub mod ir;
pub mod lexer;
pub mod parser;

pub use analyzer::{OwnershipState, SymbolTable, TypeChecker};
pub use ast::{Expr, Literal, PipelineStep, Stmt};
pub use codegen::WasmGenerator;
pub use ir::generator::generate_ir_all;
pub use ir::{BasicBlock, IrFunction, IrInstruction, IrModule};
pub use lexer::{Token, TokenKind};

pub fn compile_to_wasm(source: &str) -> Result<Vec<u8>, String> {
    let tokens = crate::lexer::tokenizer::Lexer::new(source).tokenize();
    let stmts = crate::parser::Parser::new(tokens).parse()?;
    TypeChecker::new().check(&stmts)?;
    let ir_module = generate_ir_all(&stmts)?;
    WasmGenerator::new().compile(&ir_module)
}
