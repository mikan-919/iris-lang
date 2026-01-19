use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
    TypeSection, ValType,
};

fn main() {
    // Create a simple WASM module with the add function
    let mut types = TypeSection::new();
    types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);

    let mut functions = FunctionSection::new();
    functions.function(0);

    // Create the function with 3 additional locals (t0, t1, t2)
    let mut wasm_func = Function::new(vec![(3, ValType::I32)]);

    // Instructions: a + b
    // Parameters are at local indices 0 and 1
    // Temp locals are at local indices 2, 3, 4
    wasm_func.instruction(&Instruction::LocalGet(0)); // Get a
    wasm_func.instruction(&Instruction::LocalSet(2)); // Store in t0 (local 2)
    wasm_func.instruction(&Instruction::LocalGet(1)); // Get b
    wasm_func.instruction(&Instruction::LocalSet(3)); // Store in t1 (local 3)
    wasm_func.instruction(&Instruction::LocalGet(2)); // Get t0
    wasm_func.instruction(&Instruction::LocalGet(3)); // Get t1
    wasm_func.instruction(&Instruction::I32Add); // a + b
    wasm_func.instruction(&Instruction::LocalSet(4)); // Store in t2 (local 4)
    wasm_func.instruction(&Instruction::LocalGet(4)); // Get result
    wasm_func.instruction(&Instruction::End); // End function

    let mut code = CodeSection::new();
    code.function(&wasm_func);

    let mut exports = ExportSection::new();
    exports.export("add", ExportKind::Func, 0);

    let mut module = Module::new();
    module.section(&types);
    module.section(&functions);
    module.section(&exports);
    module.section(&code);

    let wasm_bytes = module.finish();
    println!("Generated WASM ({} bytes):", wasm_bytes.len());
    println!(
        "Hex: {}",
        wasm_bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    );
}
