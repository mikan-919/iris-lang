use iris_lang::compile_to_wasm_internal;
use wasmtime::*;

#[test]
fn test_execution_add_function() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b";
    let wasm_bytes = compile_to_wasm_internal(code).expect("Compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("Failed to create WASM module");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("Failed to instantiate");

    let add_func = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "add")
        .expect("Failed to get function");

    let result = add_func
        .call(&mut store, (3, 5))
        .expect("Failed to call function");

    assert_eq!(result, 8, "add(3, 5) should return 8");
}

#[test]
fn test_execution_return_constant() {
    let code = "export fn return42() -> Int =: 42";
    let wasm_bytes = compile_to_wasm_internal(code).expect("Compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("Failed to create WASM module");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("Failed to instantiate");

    let func = instance
        .get_typed_func::<(), i32>(&mut store, "return42")
        .expect("Failed to get function");

    let result = func.call(&mut store, ()).expect("Failed to call function");

    assert_eq!(result, 42, "return42() should return 42");
}

#[test]
fn test_execution_add_negative() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b";
    let wasm_bytes = compile_to_wasm_internal(code).expect("Compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("Failed to create WASM module");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("Failed to instantiate");

    let add_func = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "add")
        .expect("Failed to get function");

    let result = add_func
        .call(&mut store, (-5, 10))
        .expect("Failed to call function");

    assert_eq!(result, 5, "add(-5, 10) should return 5");
}
