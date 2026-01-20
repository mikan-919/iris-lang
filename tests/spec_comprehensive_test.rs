use iris_lang::compile_to_wasm_internal;
use wasmtime::*;

// ============================================================================
// Section 1: Core Principles
// ============================================================================

#[test]
fn test_core_principle_expression_style_functions() {
    // Expression Style: 結果を直接定義する場合
    let code = "export fn double(n: Int) -> Int =: n * 2";
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

    assert_eq!(result, 6, "double(3) should return 6");
}

// ============================================================================
// Section 2: The Backbone (Operators)
// ============================================================================

#[test]
fn test_operator_initiate() {
    // =: Initiate - 開始・束縛
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

    assert_eq!(result, 8, "add(3, 5) should return 8");
}

#[test]
#[ignore = "Pipeline syntax not yet implemented"]
fn test_operator_next_pipeline() {
    // :: Next - 継続（パイプライン）
    let code = "export fn main() -> Int =: 10 :: * 2 :: + 5";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(
        result.is_err(),
        "Pipeline (::) should not be implemented yet"
    );
}

#[test]
fn test_operator_await() {
    // :~ Await - 非同期処理の完了を待つ
    let code = "export fn test() =: fetchData() :~ process()";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Await (:) should not be implemented yet");
}

#[test]
fn test_operator_try() {
    // :^ Try - 失敗時は即座にreturn
    let code = "export fn test() =: riskyOperation() :^ handleResult()";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Try (:) should not be implemented yet");
}

#[test]
fn test_operator_force() {
    // :! Force - 失敗時はPanic
    let code = "export fn test() =: maybeValue() :! process()";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Force (:) should not be implemented yet");
}

#[test]
fn test_operator_catch() {
    // :? Catch - 失敗をハンドル
    let code = "export fn test() =: risky() :? handleError()";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Catch (:) should not be implemented yet");
}

#[test]
fn test_operator_or() {
    // :| Or - 代替値
    let code = "export fn test() =: fallback() :| defaultValue";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Or (:) should not be implemented yet");
}

#[test]
fn test_operator_tag() {
    // :> Tag - 借用保存
    let code = "export fn test() =: value :> snapshot :: process()";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Tag (:>) should not be implemented yet");
}

#[test]
fn test_operator_join() {
    // :& Join - 簡易結合
    let code = "export fn test() =: (a, b) :& c";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Join (:) should not be implemented yet");
}

// ============================================================================
// Section 3: Syntax & Structure
// ============================================================================

#[test]
fn test_syntax_arithmetic_operations() {
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

    assert_eq!(result, 8, "add(3, 5) should return 8");
}

#[test]
fn test_syntax_multiple_operations() {
    let code = "export fn compute(a: Int, b: Int, c: Int) -> Int =: a + b * c";
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

    assert_eq!(result, 8, "3 + 5*1 should return 8");
}

#[test]
fn test_syntax_parentheses() {
    let code = "export fn compute(a: Int, b: Int) -> Int =: (a + b) * 2";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Parentheses should not be implemented yet");
}

#[test]
fn test_function_style_export() {
    let code = "export fn exported(n: Int) -> Int =: n + 1";
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

    assert_eq!(result, 4, "exported(3) should return 4");
}

// ============================================================================
// Section 4: Control Flow (Flow Structures)
// ============================================================================

#[test]
fn test_control_flow_match() {
    // Match (Branching) with ( )
    let code = "export fn test(n: Int) -> Int =: match n ( 1 =: 10 2 =: 20 _ =: 0 )";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Match should not be implemented yet");
}

#[test]
fn test_control_flow_join() {
    // Parallelism / Tuple Construction (Join)
    let code = "export fn test() =: ( count(), =: \"Label\" :: upper() )";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(result.is_err(), "Join should not be implemented yet");
}

// ============================================================================
// Section 5: Ownership & Mutation
// ============================================================================

#[test]
fn test_ownership_basic() {
    let code = "export fn pass(n: Int) -> Int =: n";
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

    assert_eq!(result, 3, "pass(3) should return 3");
}

#[test]
#[ignore = "Block body IR generation not yet implemented"]
fn test_mutation_block() {
    // Mutation with mutate { }
    let code = "export fn test() { let x =: 10 x = 20 x }";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(
        result.is_err(),
        "Mutation blocks should not be implemented yet"
    );
}

#[test]
fn test_borrowing_tag() {
    // Borrowing with :>
    let code = "export fn test() =: value :> snapshot :: process()";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(
        result.is_err(),
        "Borrowing (:) should not be implemented yet"
    );
}

// ============================================================================
// Additional Tests: Types and Literals
// ============================================================================

#[test]
fn test_type_int() {
    let code = "export fn identity(n: Int) -> Int =: n";
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

    assert_eq!(result, 3, "identity(3) should return 3");
}

#[test]
fn test_literal_positive_int() {
    let code = "export fn main() -> Int =: 42";
    let result = compile_to_wasm_internal(code);

    // Should NOT have auto-generated main since user defined it
    assert!(result.is_ok(), "Should compile successfully");
}

#[test]
fn test_literal_zero() {
    let code = "export fn main() -> Int =: 0";
    let result = compile_to_wasm_internal(code);

    assert!(result.is_ok(), "Should compile successfully");
}

#[test]
fn test_literal_negative_int() {
    let code = "export fn negate(n: Int) -> Int =: -n";
    let result = compile_to_wasm_internal(code);

    // NOT IMPLEMENTED YET - Should fail
    assert!(
        result.is_err(),
        "Negative literals should not be implemented yet"
    );
}

// ============================================================================
// Additional Tests: Multiple Functions
// ============================================================================

#[test]
#[ignore = "Pipeline syntax not yet implemented"]
fn test_multiple_functions_compilation() {
    let code = "
        fn add(a: Int, b: Int) -> Int =: a + b
        fn multiply(a: Int, b: Int) -> Int =: a * b
        export fn compute(n: Int) -> Int =: n :: add(5) :: multiply(2)
    ";
    let result = compile_to_wasm_internal(code);

    // Pipeline not implemented yet
    assert!(
        result.is_err(),
        "Pipeline syntax should not be implemented yet"
    );
}

#[test]
fn test_empty_exported_function() {
    let code = "export fn empty() -> Int =: 0";
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
