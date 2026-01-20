use iris_lang::compile_to_wasm_internal;

fn main() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b";
    match compile_to_wasm_internal(code) {
        Ok(wasm_bytes) => {
            println!("WASM bytecode length: {}", wasm_bytes.len());
            println!("WASM bytes (hex):");
            for (i, chunk) in wasm_bytes.chunks(16).enumerate() {
                print!("{:04x}: ", i * 16);
                for byte in chunk {
                    print!("{:02x} ", byte);
                }
                println!();
            }

            println!("\nWASM Header:");
            println!(
                "  Magic: {:02x} {:02x} {:02x} {:02x} (should be 00 61 73 6d)",
                wasm_bytes[0], wasm_bytes[1], wasm_bytes[2], wasm_bytes[3]
            );
            println!(
                "  Version: {:02x} {:02x} {:02x} {:02x} (should be 01 00 00 00)",
                wasm_bytes[4], wasm_bytes[5], wasm_bytes[6], wasm_bytes[7]
            );

            let mut offset = 8;
            while offset < wasm_bytes.len() {
                let section_id = wasm_bytes[offset];
                let section_len = read_u32(&wasm_bytes, offset + 1) as usize;
                println!(
                    "\nSection {} at offset {} ({} bytes):",
                    section_id, offset, section_len
                );

                match section_id {
                    1 => println!("  Type Section"),
                    3 => println!("  Function Section"),
                    7 => println!("  Export Section"),
                    10 => {
                        println!("  Code Section");
                        parse_code_section(
                            &wasm_bytes,
                            offset + 1 + section_len_size(section_len),
                            section_len,
                        );
                    }
                    _ => println!("  Unknown Section"),
                }

                offset += 1 + section_len_size(section_len) + section_len;
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let mut result = 0;
    let mut shift = 0;
    let mut i = offset;
    loop {
        let byte = bytes[i];
        result |= ((byte & 0x7F) as u32) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    result
}

fn section_len_size(len: usize) -> usize {
    let mut x = len;
    let mut count = 0;
    loop {
        x >>= 7;
        count += 1;
        if x == 0 {
            break;
        }
    }
    count
}

fn parse_code_section(bytes: &[u8], start: usize, _len: usize) {
    println!("    Parsing code section...");
    let mut offset = start;

    let func_count = read_u32(bytes, offset) as usize;
    println!("    Function count: {}", func_count);
    offset += section_len_size(func_count);

    for func_idx in 0..func_count {
        println!("\n    Function {}:", func_idx);
        let func_len = read_u32(bytes, offset) as usize;
        offset += section_len_size(func_len);

        let locals_count = read_u32(bytes, offset) as usize;
        println!("      Locals entries: {}", locals_count);
        offset += section_len_size(locals_count);

        for i in 0..locals_count {
            let count = read_u32(bytes, offset);
            offset += section_len_size(count as usize);
            let ty = bytes[offset];
            offset += 1;
            println!("      Local entry {}: count={}, type={}", i, count, ty);
        }

        println!("      Instructions:");
        while offset < start + func_len {
            let opcode = bytes[offset];
            print!("        opcode={:02x} ", opcode);

            match opcode {
                0x20 => {
                    offset += 1;
                    let idx = read_u32(bytes, offset);
                    offset += section_len_size(idx as usize);
                    println!("local.get {}", idx);
                }
                0x21 => {
                    offset += 1;
                    let idx = read_u32(bytes, offset);
                    offset += section_len_size(idx as usize);
                    println!("local.set {}", idx);
                }
                0x41 => {
                    offset += 1;
                    let val = read_i32(bytes, offset);
                    offset += section_len_size(val.unsigned_abs() as usize);
                    println!("i32.const {}", val);
                }
                0x6a => {
                    offset += 1;
                    println!("i32.add");
                }
                0x0b => {
                    offset += 1;
                    println!("end");
                    break;
                }
                _ => {
                    offset += 1;
                    println!("unknown");
                }
            }
        }
    }
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    let mut result: i32 = 0;
    let mut shift: u32 = 0;
    let mut i = offset;
    loop {
        let byte = bytes[i];
        result |= ((byte & 0x7F) as i32) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            if shift < 31 && (byte & 0x40) != 0 {
                result |= !0 << (shift + 7);
            }
            break;
        }
        shift += 7;
    }
    result
}
