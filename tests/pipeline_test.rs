use iris_lang::compile_to_wasm_internal;
use wasmtime::*;

#[test]
fn test_pipeline_syntax() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b\nexport fn main()->Int=:3::add(3)";
    let result = compile_to_wasm_internal(code);

    println!("Compilation result: {:?}", result);

    if let Ok(wasm_bytes) = result {
        let engine = Engine::default();
        match Module::new(&engine, &wasm_bytes) {
            Ok(module) => {
                let mut store = Store::new(&engine, ());
                match Instance::new(&mut store, &module, &[]) {
                    Ok(instance) => {
                        let main_func = instance.get_typed_func::<(), i32>(&mut store, "main");
                        println!("main function available: {:?}", main_func.is_ok());

                        if let Ok(func) = main_func {
                            match func.call(&mut store, ()) {
                                Ok(result) => {
                                    println!("main() returned: {}", result);
                                }
                                Err(e) => {
                                    eprintln!("Error calling main: {:?}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to instantiate: {:?}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to create module: {:?}", e);
            }
        }
    } else {
        eprintln!("Compilation failed: {:?}", result);
    }
}
