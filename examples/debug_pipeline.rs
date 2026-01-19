use iris_lang::compile_to_wasm_internal;
use iris_lang::ir::generator::generate_ir_all;
use iris_lang::lexer::tokenizer::Lexer;
use iris_lang::parser::Parser;

fn main() {
    let code = "export fn add(a: Int, b: Int) -> Int =: a + b\nexport fn main()->Int=:3::add(3)";

    println!("Source code:\n{}", code);
    println!();

    let tokens = Lexer::new(code).tokenize();
    let stmts = Parser::new(tokens).parse().unwrap();

    let ir_module = generate_ir_all(&stmts).unwrap();

    println!("Generated IR Module:");
    for (i, func) in ir_module.functions.iter().enumerate() {
        println!("  Function {}: {}", i, func.name);
        println!("    Exported: {}", func.is_exported);
        println!("    Params: {:?}", func.params);
        println!("    Return type: {:?}", func.return_type);
        println!("    Instructions:");
        for (j, instr) in func.block.instructions.iter().enumerate() {
            println!("      {}: {:?}", j, instr);
        }
        println!();
    }

    match compile_to_wasm_internal(code) {
        Ok(wasm_bytes) => {
            println!("WASM bytecode length: {}", wasm_bytes.len());

            use wasmtime::*;
            let engine = Engine::default();
            let module = Module::new(&engine, &wasm_bytes);
            match module {
                Ok(_) => {
                    let mut store = Store::new(&engine, ());
                    let instance = Instance::new(&mut store, &module.unwrap(), &[]);
                    match instance {
                        Ok(inst) => {
                            let main_func = inst.get_typed_func::<(), i32>(&mut store, "main");
                            match main_func {
                                Ok(func) => match func.call(&mut store, ()) {
                                    Ok(result) => {
                                        println!("main() returned: {}", result);
                                    }
                                    Err(e) => {
                                        eprintln!("Error calling main: {:?}", e);
                                    }
                                },
                                Err(e) => {
                                    eprintln!("Failed to get main function: {:?}", e);
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
        }
        Err(e) => {
            eprintln!("Compilation failed: {}", e);
        }
    }
}
