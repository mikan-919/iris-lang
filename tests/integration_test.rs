use iris_lang::compile_to_wasm_internal;

#[test]
fn test_compile_add_function() {
    let code = "fn add(a: Int, b: Int) -> Int =: a + b";
    let result = compile_to_wasm_internal(code);

    assert!(result.is_ok(), "Compilation failed: {:?}", result);

    let wasm_bytes = result.unwrap();

    // Check for WebAssembly magic number
    assert_eq!(&wasm_bytes[0..4], b"\x00\x61\x73\x6d");
    assert_eq!(&wasm_bytes[4..8], b"\x01\x00\x00\x00");
}

#[test]
fn test_compile_add_function_with_export() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b";
    let result = compile_to_wasm_internal(code);

    assert!(result.is_ok(), "Compilation failed: {:?}", result);

    let wasm_bytes = result.unwrap();

    // Check for WebAssembly magic number
    assert_eq!(&wasm_bytes[0..4], b"\x00\x61\x73\x6d");
    assert_eq!(&wasm_bytes[4..8], b"\x01\x00\x00\x00");
}

#[test]
fn test_compile_add_function_no_types() {
    let code = "fn add(a, b) =: a + b";
    let result = compile_to_wasm_internal(code);

    // Type Any is not yet supported, so this should fail
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Type Any not yet supported"));
}

#[test]
fn test_compile_return_constant() {
    let code = "fn return42() -> Int =: 42";
    let result = compile_to_wasm_internal(code);

    assert!(result.is_ok(), "Compilation failed: {:?}", result);

    let wasm_bytes = result.unwrap();

    // Check for WebAssembly magic number
    assert_eq!(&wasm_bytes[0..4], b"\x00\x61\x73\x6d");
    assert_eq!(&wasm_bytes[4..8], b"\x01\x00\x00\x00");
}
