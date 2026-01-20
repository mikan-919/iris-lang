use iris_lang::{
    ir::BasicBlock, ir::IrInstruction, ir::generator::generate_ir_for_function,
    lexer::tokenizer::Lexer, parser::Parser,
};

fn collect_temp_regs(block: &BasicBlock) -> Vec<String> {
    let mut temp_regs = std::collections::HashSet::new();
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

    println!("\nExpected locals:");
    println!("  Parameters: {} locals", func.params.len());
    println!("  Temp regs: {} locals", temp_regs.len());
    println!("  Total: {} locals", func.params.len() + temp_regs.len());
}
