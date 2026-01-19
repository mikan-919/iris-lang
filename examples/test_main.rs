use iris_lang::compile_to_wasm_internal;
use wasmtime::*;

fn main() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b";
    let wasm_bytes = compile_to_wasm_internal(code).expect("Compilation failed");

    println!("WASM bytecode length: {}", wasm_bytes.len());

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm_bytes).expect("Failed to create WASM module");
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).expect("Failed to instantiate");

    println!("\nCalling main() function:");
    let main_func = instance
        .get_typed_func::<(), i32>(&mut store, "main")
        .expect("Failed to get main function");

    let result = main_func
        .call(&mut store, ())
        .expect("Failed to call main function");
    println!("main() returned: {}", result);

    println!("\nCalling add(3, 5) directly:");
    let add_func = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "add")
        .expect("Failed to get add function");

    let add_result = add_func
        .call(&mut store, (3, 5))
        .expect("Failed to call add function");
    println!("add(3, 5) returned: {}", add_result);
}
