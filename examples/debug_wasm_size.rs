use iris_lang::compile_to_wasm_internal;

fn main() {
    let code = "fn add(a: Int, b: Int) -> Int =: a + b";
    let result = compile_to_wasm_internal(code);

    match result {
        Ok(bytes) => {
            println!("Compilation successful!");
            println!("Wasm bytecode length: {} bytes", bytes.len());
            println!(
                "Magic: {:?} (should be b\"\\x00\\x61\\x73\\x6d\")",
                &bytes[0..4]
            );
            println!(
                "Version: {:?} (should be b\"\\x01\\x00\\x00\\x00\")",
                &bytes[4..8]
            );
            println!(
                "All bytes (hex): {}",
                bytes
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            std::process::exit(1);
        }
    }
}
