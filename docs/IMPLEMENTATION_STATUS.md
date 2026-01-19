# Iris Language Implementation Status

## Overview
This document tracks the implementation status of all features specified in IRIS_SPEC.md (v1.1).

**Last Updated**: 2026-01-19

---

## Summary

| Category | Total Features | Implemented | Not Implemented | Progress |
|----------|---------------|--------------|-----------------|----------|
| Core Principles | 3 | 2 | 1 | 67% |
| The Backbone (Operators) | 9 | 1 | 8 | 11% |
| Syntax & Structure | 4 | 2 | 2 | 50% |
| Control Flow | 2 | 0 | 2 | 0% |
| Ownership & Mutation | 3 | 1 | 2 | 33% |
| Types & Literals | 4 | 3 | 1 | 75% |
| **Total** | **25** | **9** | **16** | **36%** |

---

## Section 1: Core Principles

| Feature | Status | Notes |
|---------|--------|-------|
| Verticality | ✅ Implemented | 2-character operators are recognized |
| Subject-First | ✅ Implemented | Functions always start with subject |
| Flow Structure vs Procedural Block | ⚠️ Partial | Expression style works, block style not implemented |
| Operator-driven Semantics | ❌ Not Implemented | Only basic operators work |

---

## Section 2: The Backbone (Operators)

| Operator | Name | Status | Notes |
|----------|-------|--------|-------|
| `=:` | Initiate | ✅ Implemented | Basic function definition works |
| `::` | Next | ❌ Not Implemented | Pipeline syntax causes lexer errors |
| `:~` | Await | ❌ Not Implemented | Async operations not supported |
| `:^` | Try | ❌ Not Implemented | Error propagation not implemented |
| `:!` | Force | ❌ Not Implemented | Panic on error not implemented |
| `:?` | Catch | ❌ Not Implemented | Error handling not implemented |
| `:|` | Or | ❌ Not Implemented | Default values not implemented |
| `:>` | Tag | ❌ Not Implemented | Borrowing not implemented |
| `:&` | Join | ❌ Not Implemented | Tuple operations not implemented |

---

## Section 3: Syntax & Structure

| Feature | Status | Notes |
|---------|--------|-------|
| Pipeline Definition | ❌ Not Implemented | `::` operator not supported |
| Function Styles | ⚠️ Partial | Expression style works, procedural style not implemented |
| Arithmetic Operations | ✅ Implemented | `+`, `-`, `*`, `/` work |
| Export keyword | ✅ Implemented | Functions can be exported |

---

## Section 4: Control Flow (Flow Structures)

| Feature | Status | Notes |
|---------|--------|-------|
| Match (Branching) | ❌ Not Implemented | Pattern matching not supported |
| Join (Parallelism/Tuple) | ❌ Not Implemented | Tuple operations not supported |

---

## Section 5: Ownership & Mutation

| Feature | Status | Notes |
|---------|--------|-------|
| Move by Default | ✅ Implemented | Parameters are moved |
| Mutation | ❌ Not Implemented | `mutate { }` blocks not supported |
| Borrowing | ❌ Not Implemented | No borrowing mechanism |

---

## Types & Literals

| Type/Literal | Status | Notes |
|--------------|--------|-------|
| Int | ✅ Implemented | 32-bit integers |
| Float | ❌ Not Implemented | 64-bit floats not supported |
| Bool | ⚠️ Partial | Type checker recognizes it, not fully tested |
| String | ❌ Not Implemented | String literals not supported |
| Positive Int Literal | ✅ Implemented | Works |
| Zero | ✅ Implemented | Works |
| Negative Int Literal | ❌ Not Implemented | Unary minus not supported |
| Unit | ❌ Not Implemented | `()` type not supported |
| Any | ❌ Not Implemented | Type inference not implemented |

---

## Additional Features

| Feature | Status | Notes |
|---------|--------|-------|
| WASM Compilation | ✅ Implemented | Successfully compiles to WASM |
| WASM Execution | ✅ Implemented | Runs via wasmtime |
| Type Checking | ✅ Implemented | Basic type checker works |
| Auto-generated main | ✅ Implemented | Creates main() for exported functions with params |

---

## Test Results

All tests in `tests/spec_comprehensive_test.rs`:

- **Total Tests**: 25
- **Passing**: 22 (expected to pass)
- **Failing**: 3 (expected to fail - features not implemented)
- **Unexpected Failures**: 0

### Failing Tests (Expected)

1. `test_multiple_functions_compilation` - Pipeline syntax not implemented
2. `test_mutation_block` - Mutation blocks not implemented
3. `test_operator_next_pipeline` - Pipeline operator not implemented

### Passing Tests

All other tests pass, demonstrating working:
- Basic arithmetic operations
- Function definitions and exports
- Int type and literals
- WASM compilation and execution
- Auto-generated main function

---

## Known Limitations

1. **Pipeline Syntax**: The `::` operator is recognized by the parser but not fully implemented in the IR generation.
2. **Block Bodies**: Only expression-style functions work. Procedural blocks `{ }` are not supported.
3. **Types**: Only `Int` type is fully implemented. `Float`, `Bool`, `String`, `Unit` are not.
4. **Control Flow**: No `match`, `if`, or loop constructs.
5. **Error Handling**: No `try`, `catch`, or error propagation.
6. **Async**: No support for async operations or await.
7. **Advanced Operators**: No `:>`, `:|`, `:^`, `:~`, `:?`, `:&` operators.
8. **Borrowing**: No borrowing or mutation support.

---

## Next Steps

### Priority 1: Core Language Features
- [ ] Implement pipeline operator `::` for function chaining
- [ ] Implement procedural block syntax `{ }`
- [ ] Support `Float` type and operations

### Priority 2: Control Flow
- [ ] Implement `match` expression with patterns
- [ ] Implement `if`/`else` control flow
- [ ] Implement basic loops (e.g., `for`)

### Priority 3: Advanced Features
- [ ] Implement error handling with `:`, `:?`
- [ ] Implement borrowing with `:>`
- [ ] Support tuple operations with `:&`
- [ ] Add async support with `:~`

### Priority 4: Type System
- [ ] Implement type inference for `Any` type
- [ ] Support `Bool` type fully
- [ ] Support `String` type and operations
- [ ] Support `Unit` type

---

## References

- [IRIS_SPEC.md](./IRIS_SPEC.md) - Language specification
- [tests/spec_comprehensive_test.rs](../tests/spec_comprehensive_test.rs) - Comprehensive test suite
