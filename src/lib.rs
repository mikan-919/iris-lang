use wasm_bindgen::prelude::*;

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

#[wasm_bindgen]
pub fn compile_to_wasm(source: &str) -> Vec<u8> {
    match compile_to_wasm_internal(source) {
        Ok(wasm_bytes) => wasm_bytes,
        Err(error) => {
            let error_msg = format!("ERROR: {}", error);
            error_msg.into_bytes()
        }
    }
}

pub fn compile_to_wasm_internal(source: &str) -> Result<Vec<u8>, String> {
    let tokens = crate::lexer::tokenizer::Lexer::new(source).tokenize();
    let stmts = crate::parser::Parser::new(tokens).parse()?;
    TypeChecker::new().check(&stmts)?;
    let mut ir_module = generate_ir_all(&stmts)?;
    add_main_function(&mut ir_module)?;
    WasmGenerator::new().compile(&ir_module)
}

fn add_main_function(ir_module: &mut IrModule) -> Result<(), String> {
    use crate::ast::{Literal, Type};
    use crate::ir::{BasicBlock, IrFunction, IrInstruction};

    let has_main = ir_module.functions.iter().any(|f| f.name == "main");

    if has_main {
        return Ok(());
    }

    let exported_funcs: Vec<_> = ir_module
        .functions
        .iter()
        .filter(|f| f.is_exported && !f.params.is_empty())
        .cloned()
        .collect();

    if !exported_funcs.is_empty() {
        let func = &exported_funcs[0];

        let mut block = BasicBlock::new();
        let mut temp_offset = 0;
        let mut args = Vec::new();

        for (param_idx, _) in func.params.iter().enumerate() {
            let test_value = match param_idx {
                0 => 3,
                1 => 5,
                _ => 1,
            };
            let reg = format!("t{}", temp_offset);
            temp_offset += 1;
            args.push(reg.clone());
            block.add(IrInstruction::LoadConst {
                reg: reg.clone(),
                value: Expr::Literal(Literal::Integer(test_value)),
            });
        }

        block.add(IrInstruction::Call {
            func_name: func.name.clone(),
            args,
            target: "result".to_string(),
        });

        block.add(IrInstruction::Return {
            reg: "result".to_string(),
        });

        let main_func = IrFunction {
            name: "main".to_string(),
            is_exported: true,
            is_external: false,
            external_name: None,
            params: vec![],
            return_type: Type::Int,
            block,
        };

        ir_module.functions.push(main_func);
    }

    Ok(())
}
