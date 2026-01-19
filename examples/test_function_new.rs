use wasm_encoder::{CodeSection, Function, Instruction, ValType};

fn main() {
    // Test 1: Parameters + 3 temp locals
    let func1 = Function::new(vec![(3, ValType::I32)]);
    println!("Test 1: 2 params + 3 temp locals");
    let mut code = CodeSection::new();
    code.function(&func1);
    let mut bytes = Vec::new();
    code.write(&mut bytes);
    println!("  Code section bytes: {:?}", bytes);

    // Test 2: Multiple groups of locals
    let func2 = Function::new(vec![(1, ValType::I32), (2, ValType::I32)]);
    println!("\nTest 2: Multiple groups");
    let mut code = CodeSection::new();
    code.function(&func2);
    let mut bytes = Vec::new();
    code.write(&mut bytes);
    println!("  Code section bytes: {:?}", bytes);
}
