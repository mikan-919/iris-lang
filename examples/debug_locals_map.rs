use iris_lang::{
    ir::BasicBlock, ir::IrInstruction, ir::generator::generate_ir_for_function,
    lexer::tokenizer::Lexer, parser::Parser,
};
use std::collections::HashSet;

fn collect_temp_regs(block: &BasicBlock) -> Vec<String> {
    let mut temp_regs = HashSet::new();
    for instr in &block.instructions {
        match instr {
            IrInstruction::LoadConst { reg, .. } => {
                temp_regs.insert(reg.clone());
            }
            IrInstruction::LoadLocal { reg, .. } => {
                temp_regs.insert(reg.clone());
            }
            IrInstruction::BinaryOp {
                left,
                right,
                target,
                ..
            } => {
                temp_regs.insert(left.clone());
                temp_regs.insert(right.clone());
                temp_regs.insert(target.clone());
            }
            IrInstruction::Call { args, target, .. } => {
                for arg in args {
                    temp_regs.insert(arg.clone());
                }
                temp_regs.insert(target.clone());
            }
            IrInstruction::Assign { var: _, reg } => {
                temp_regs.insert(reg.clone());
            }
            IrInstruction::Return { reg } => {
                temp_regs.insert(reg.clone());
            }
            IrInstruction::Branch { cond, .. } => {
                temp_regs.insert(cond.clone());
            }
            IrInstruction::Exit { value } => {
                temp_regs.insert(value.clone());
            }
            IrInstruction::Panic { .. } => {}
        }
    }
    let mut sorted: Vec<_> = temp_regs.into_iter().collect();
    sorted.sort();
    sorted
}

fn main() {
    let code = "fn add(a: Int, b: Int) -> Int =: a + b";

    let tokens = Lexer::new(code).tokenize();
    let stmts = Parser::new(tokens).parse().expect("Parse failed");

    let func = generate_ir_for_function(&stmts[0]).expect("IR generation failed");

    println!("Function: {}", func.name);
    println!("Parameters: {:?}", func.params);
    println!("Instructions:");
    for (i, instr) in func.block.instructions.iter().enumerate() {
        println!("  [{}]: {:?}", i, instr);
    }

    let temp_regs = collect_temp_regs(&func.block);
    println!("\nCollected temporary registers: {:?}", temp_regs);
    println!("Number of temporary registers: {}", temp_regs.len());

    // Simulate locals_map creation
    println!("\n=== Locals Map Construction ===");
    let mut all_locals: Vec<(String, u32)> = Vec::new();
    for (param_idx, (param_name, _)) in func.params.iter().enumerate() {
        println!("Param: {} -> index {}", param_name, param_idx);
        all_locals.push((param_name.clone(), param_idx as u32));
    }

    for reg in &temp_regs {
        let idx = all_locals.len() as u32;
        println!("Temp reg: {} -> index {}", reg, idx);
        all_locals.push((reg.clone(), idx));
    }

    println!("\nAll locals: {:?}", all_locals);

    // Count additional locals
    let additional_count = all_locals
        .iter()
        .filter(|(name, _)| !func.params.iter().any(|(param_name, _)| param_name == name))
        .count();
    println!("Additional locals count: {}", additional_count);
}
