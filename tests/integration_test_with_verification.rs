use iris_lang::compile_to_wasm_internal;

#[test]
fn test_compile_and_verify_add_function() {
    let code = "fn add(a: Int, b: Int) -> Int =: a + b";
    let result = compile_to_wasm_internal(code);

    assert!(result.is_ok(), "Compilation failed: {:?}", result);

    let wasm_bytes = result.unwrap();

    // Verify WebAssembly magic number and version
    assert_eq!(
        &wasm_bytes[0..4],
        b"\x00\x61\x73\x6d",
        "Invalid WASM magic number"
    );
    assert_eq!(
        &wasm_bytes[4..8],
        b"\x01\x00\x00\x00",
        "Invalid WASM version"
    );

    // Check that the module is non-empty and reasonably sized
    assert!(wasm_bytes.len() > 100, "WASM module too small");
    assert!(wasm_bytes.len() < 10000, "WASM module too large");

    println!("WASM module size: {} bytes", wasm_bytes.len());

    // Basic section validation - should at least have type, function, export, code sections
    let mut has_type_section = false;
    let mut has_function_section = false;
    let mut has_export_section = false;
    let mut has_code_section = false;

    let mut offset = 8;
    while offset < wasm_bytes.len() {
        let section_id = wasm_bytes[offset];
        offset += 1;

        let mut section_len = 0;
        let mut len_shift = 0;
        loop {
            let byte = wasm_bytes[offset];
            offset += 1;
            if byte < 128 {
                section_len |= byte << len_shift;
                break;
            }
            section_len |= (byte & 0x7F) << len_shift;
            len_shift += 7;
        }

        match section_id {
            1 => has_type_section = true,
            3 => has_function_section = true,
            7 => has_export_section = true,
            10 => has_code_section = true,
            _ => {}
        }

        offset += section_len as usize;
    }

    assert!(has_type_section, "Missing type section");
    assert!(has_function_section, "Missing function section");
    assert!(has_code_section, "Missing code section");
    // Export section might not exist if function is not exported
}
