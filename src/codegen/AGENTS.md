# CODEGEN MODULE

## OVERVIEW
Wasm code generation from IR using wasm_encoder crate, produces valid .wasm binary.

## STRUCTURE
- `WasmGenerator::compile()` - Main entry, generates complete Wasm module sections
- `type_to_valtype()` - Maps IR types to Wasm ValType
- `encode_function_body()` - Builds locals and encodes instruction sequence
- `encode_instruction()` - Converts IrInstruction to Wasm instructions

## WHERE TO LOOK
- Entry point: `compile()` (line 20) - orchestrates Type/Function/Export/Code sections
- Type mapping: `type_to_valtype()` (line 75) - Int→I32, Float→F64, Bool/Unit→I32
- Instruction encoding: `encode_instruction()` (line 182) - BinaryOp, Call, Return, etc.
- Test coverage: `test_simple_add_function()`, `test_type_to_valtype()` (line 328)

## CONVENTIONS
- Section order: Type → Function → Export → Code (as required by Wasm spec)
- Local indices: function params first, then temporary registers (line 132-144)
- Binary ops: use signed integer variants (I32DivS, not I32DivU)
- Always end function body with `Instruction::End` (line 163)

## ANTI-PATTERNS
- Don't assume type exists: `type_to_valtype()` returns Result for unsupported types
- Don't hardcode Wasm magic numbers: use `Module::new()` and `Module::finish()`
- Don't skip locals_map construction: all registers (params + temps) need indices
- Don't forget to check `func_indices.get()` before `Instruction::Call` (line 228)
