use wasm_encoder::{Encode, Function, Instruction, ValType};

fn main() {
    println!("=== Test 1: Vec<(3, I32)> ===");
    let func1 = Function::new(vec![(3u32, ValType::I32)]);

    let mut bytes = Vec::new();
    // Manually encode to see what Function does internally
    // Based on source, Function::new encodes locals first
    let locals_len = 3u32;
    let mut local_bytes = Vec::new();
    locals_len.encode(&mut local_bytes);
    ValType::I32.encode(&mut local_bytes);
    println!("Local declaration bytes: {:?}", local_bytes);

    // Now encode some instructions
    Instruction::LocalGet(0).encode(&mut bytes);
    Instruction::LocalSet(2).encode(&mut bytes);
    Instruction::LocalGet(1).encode(&mut bytes);
    Instruction::LocalSet(3).encode(&mut bytes);

    println!("Instruction bytes: {:?}", bytes);
    println!(
        "Full function bytes (locals + instructions): {:?}",
        [&local_bytes[..], &bytes[..]]
    );
}
