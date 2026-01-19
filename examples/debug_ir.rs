use iris_lang::{ir::generator::generate_ir_for_function, lexer::tokenizer::Lexer, parser::Parser};

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
}
