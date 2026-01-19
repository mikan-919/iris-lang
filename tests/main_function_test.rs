use iris_lang::compile_to_wasm_internal;
use wasmtime::*;

#[test]
fn test_main_function_add() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b";
    let wasm_bytes = compile_to_wasm_internal(code).expect("Compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("Failed to create WASM module");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("Failed to instantiate");

    let main_func = instance
        .get_typed_func::<(), i32>(&mut store, "main")
        .expect("Failed to get main function");

    let result = main_func
        .call(&mut store, ())
        .expect("Failed to call main function");

    assert_eq!(result, 8, "main() should return 8 (add(3, 5))");
}

#[test]
fn test_main_function_return42() {
    let code = "export fn return42() -> Int =: 42";
    let wasm_bytes = compile_to_wasm_internal(code).expect("Compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("Failed to create WASM module");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("Failed to instantiate");

    let has_main = instance
        .get_typed_func::<(), i32>(&mut store, "main")
        .is_ok();

    assert!(
        !has_main,
        "main() should not be generated for functions with no params"
    );
}

#[test]
fn test_main_function_multiple_params() {
    let code = "export fn sum_three(a: Int, b: Int, c: Int) -> Int =: a + b + c";
    let wasm_bytes = compile_to_wasm_internal(code).expect("Compilation failed");

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("Failed to create WASM module");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("Failed to instantiate");

    let main_func = instance
        .get_typed_func::<(), i32>(&mut store, "main")
        .expect("Failed to get main function");

    let result = main_func
        .call(&mut store, ())
        .expect("Failed to call main function");

    assert_eq!(result, 9, "main() should return 9 (3+5+1)");
}
