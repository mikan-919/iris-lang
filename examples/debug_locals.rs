use iris_lang::compile_to_wasm_internal;

fn main() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b";
    let result = compile_to_wasm_internal(code);

    match result {
        Ok(bytes) => {
            println!("Compilation successful!");
            println!("Wasm bytecode length: {} bytes", bytes.len());
            println!(
                "All bytes (hex): {}",
                bytes
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ")
            );

            // Parse sections to understand structure
            println!("\n=== Section Analysis ===");
            let mut offset = 8;
            while offset < bytes.len() {
                let section_id = bytes[offset];
                offset += 1;

                let mut section_len = 0;
                let mut len_shift = 0;
                loop {
                    let byte = bytes[offset];
                    offset += 1;
                    if byte < 128 {
                        section_len |= byte << len_shift;
                        break;
                    }
                    section_len |= (byte & 0x7F) << len_shift;
                    len_shift += 7;
                }

                let section_start = offset;
                let section_data = &bytes[offset..offset + section_len as usize];
                offset += section_len as usize;

                let section_names = [
                    "Custom", "Type", "Import", "Function", "Table", "Memory", "Global", "Export",
                    "Start", "Element", "Code", "Data",
                ];
                let section_name = section_names.get(section_id as usize).unwrap_or(&"Unknown");

                println!(
                    "Section {} ({}): {} bytes",
                    section_id, section_name, section_len
                );

                // Detailed analysis of key sections
                match section_id {
                    1 => {
                        // Type section
                        println!("  Type section content: {:?}", section_data);
                    }
                    10 => {
                        // Code section
                        println!(
                            "  Code section first 20 bytes: {:?}",
                            &section_data[..20.min(section_data.len())]
                        );
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            std::process::exit(1);
        }
    }
}
