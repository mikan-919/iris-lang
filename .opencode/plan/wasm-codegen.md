# Work Plan: Wasm Backend Codegen

## Goal
Implement WebAssembly code generation for Iris compiler, focusing on:
- Linear code execution (no branches/control flow)
- Integer arithmetic operations (`+`, `-`, `*`, `/`)
- Function exports via `export fn`
- End-to-end compilation from source to `.wasm` binary

## Context Summary

### Current State
- **Branch**: `vk/d21d-wasm` (short-lived work branch per AGENTS.md)
- **Tests**: 29 passing (lexer, parser, analyzer, IR)
- **Dependencies**: None (clean slate)
- **Existing Modules**: lexer, parser, analyzer, ir, ast
- **IR Status**: Only handles `Stmt::Binding`, no function definitions

### User Requirements
1. **Storage**: Use Wasm locals only (no memory management yet)
2. **Arithmetic**: `:: +`, `:: -`, etc. → native `i32.add`, `i32.sub`, etc.
3. **Exports**: Only `export fn` functions should be exported
4. **Return**: `let` does `local.set` only, function ends with explicit `Return`
5. **Scope**: Internal function calls only (no FFI/imports)

### Architecture Decisions
- **Register mapping**: IR registers (t0, t1...) → Wasm locals (1:1 mapping)
- **Functions**: One Iris `fn` → One Wasm function
- **Pipeline arithmetic**: `10 :: +(5)` → loads 10, loads 5, `i32.add`
- **Type mapping**: `Type::Int` → `i32`, others return error

---

## Implementation Phases

### Phase 1: Lexer Extensions (Arithmetic Operators)
**File**: `src/lexer/mod.rs`, `src/lexer/tokenizer.rs`

**Changes**:
1. Add token kinds:
   - `TokenKind::Plus`
   - `TokenKind::Minus`
   - `TokenKind::Star`
   - `TokenKind::Slash`

2. Update tokenizer (lines 70-120 area):
   - Handle `+` character → `TokenKind::Plus`
   - Handle `-` character → `TokenKind::Minus` (distinguish from `->`)
   - Handle `*` character → `TokenKind::Star`
   - Handle `/` character → `TokenKind::Slash`

**Tests**: Update `src/lexer/tokenizer_tests.rs` with arithmetic token tests

**Commit**: `feat(lexer): add arithmetic operator tokens (+ - * /)`

---

### Phase 2: AST Extensions
**File**: `src/ast/mod.rs`

**Changes**:

1. Add `BinaryOp` enum:
```rust
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}
```

2. Update `PipelineStep` enum:
```rust
pub enum PipelineStep {
    // ... existing variants ...
    ArithmeticBinaryOp {
        op: BinaryOp,
        right: Box<Expr>,
    },
}
```

3. Update `Stmt::FunctionDefinition`:
```rust
FunctionDefinition {
    name: String,
    is_exported: bool,  // ← NEW
    params: Vec<(String, Type)>,
    return_type: Type,
    body: Box<Expr>,
}
```

4. Add `to_wasm_type` method to `Type`:
```rust
impl Type {
    pub fn to_wasm_type(&self) -> Result<wasm_encoder::ValType, String> {
        match self {
            Type::Int => Ok(wasm_encoder::ValType::I32),
            Type::Float => Ok(wasm_encoder::ValType::F64),
            Type::Bool => Ok(wasm_encoder::ValType::I32),
            _ => Err(format!("Type {} not yet supported in Wasm", self)),
        }
    }
}
```

**Tests**: Add unit tests for `to_wasm_type()`

**Commit**: `feat(ast): add binary operator support and export flag`

---

### Phase 3: Parser Extensions
**File**: `src/parser/mod.rs`

**Changes**:

1. Update `parse_statement()` (line 36-58):
```rust
TokenKind::Export => {
    self.advance();
    if self.check(TokenKind::Fn) {
        self.parse_function_definition(true)  // ← pass is_exported
    } else {
        Err(format!("expected 'fn' after 'export'"))
    }
}
TokenKind::Fn => self.parse_function_definition(false),  // ← pass is_exported
```

2. Update `parse_function_definition()` (line 72-113):
```rust
fn parse_function_definition(&mut self, is_exported: bool) -> Result<Stmt, String> {
    // ... existing parsing ...
    Ok(Stmt::FunctionDefinition {
        name,
        is_exported,  // ← NEW
        params,
        return_type,
        body: Box::new(expr),
    })
}
```

3. Update `parse_expression()` `TokenKind::Next` handling (line 126-143):
```rust
TokenKind::Next => {
    self.advance();
    match self.peek().kind {
        TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash => {
            let op = match self.advance().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                _ => unreachable!(),
            };
            let right_operand = self.parse_term()?;
            steps.push(PipelineStep::ArithmeticBinaryOp {
                op,
                right: Box::new(right_operand),
            });
        }
        // ... rest of existing handling
    }
}
```

**Tests**: Update existing function definition tests to handle `is_exported` field

**Commit**: `feat(parser): support arithmetic in pipelines and export tracking`

---

### Phase 4: IR Extensions
**File**: `src/ir/mod.rs`

**Changes**:

1. Add `BinaryOp` enum (or reuse from ast):
```rust
pub use crate::ast::BinaryOp;
```

2. Add `IrInstruction::BinaryOp` variant:
```rust
BinaryOp {
    op: BinaryOp,
    left: String,
    right: String,
    target: String,
},
```

3. Add `IrFunction` struct:
```rust
#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub is_exported: bool,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub block: BasicBlock,
}
```

4. Add `IrModule` struct:
```rust
#[derive(Debug, Clone)]
pub struct IrModule {
    pub functions: Vec<IrFunction>,
}

impl IrModule {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
        }
    }
}
```

**Tests**: Add unit tests for new IR types

**Commit**: `feat(ir): add IrFunction and BinaryOp instruction support`

---

### Phase 5: IR Generator Extensions
**File**: `src/ir/generator.rs`

**Changes**:

1. Add function to generate IR for function definitions:
```rust
pub fn generate_ir_for_function(stmt: &Stmt) -> Result<IrFunction, String> {
    if let Stmt::FunctionDefinition {
        name,
        is_exported,
        params,
        return_type,
        body,
    } = stmt
    {
        let (result_reg, mut instructions) = generate_expr_ir(body);

        // Add return instruction
        instructions.push(IrInstruction::Return { reg: result_reg });

        let block = BasicBlock {
            label: Some("entry".to_string()),
            instructions,
        };

        Ok(IrFunction {
            name: name.clone(),
            *is_exported,
            params: params.clone(),
            return_type: return_type.clone(),
            block,
        })
    } else {
        Err("Expected FunctionDefinition".to_string())
    }
}
```

2. Add function to generate IR for all statements:
```rust
pub fn generate_ir_all(stmts: &[Stmt]) -> Result<IrModule, String> {
    let mut module = IrModule::new();

    for stmt in stmts {
        if matches!(stmt, Stmt::FunctionDefinition { .. }) {
            module.functions.push(generate_ir_for_function(stmt)?);
        }
    }

    Ok(module)
}
```

3. Update `generate_expr_ir()` to handle arithmetic:
```rust
PipelineStep::ArithmeticBinaryOp { op, right } => {
    let (right_reg, right_instrs) = generate_expr_ir(right);
    instructions.extend(right_instrs);

    let result_reg = format!("t{}", instructions.len());
    instructions.push(IrInstruction::BinaryOp {
        op: op.clone(),
        left: current_reg,
        right: right_reg,
        target: result_reg.clone(),
    });
    current_reg = result_reg;
}
```

**Tests**: Update existing tests, add tests for function definition IR generation

**Commit**: `feat(ir-generator): support function definitions and arithmetic`

---

### Phase 6: Dependencies
**File**: `Cargo.toml`

**Changes**:
```toml
[dependencies]
wasm-encoder = "0.217"

[dev-dependencies]
wasmtime = "27"
```

**Commit**: `chore: add wasm-encoder and wasmtime dependencies`

---

### Phase 7: Wasm Codegen Module
**File**: `src/codegen/mod.rs`

**Changes**:

Create complete `WasmGenerator` module:

```rust
use wasm_encoder::*;
use crate::ast::{BinaryOp, Type};
use crate::ir::{IrModule, IrFunction, IrInstruction};
use std::collections::HashMap;

pub struct WasmGenerator {
    module: Module,
    func_indices: HashMap<String, u32>,
}

impl WasmGenerator {
    pub fn generate(ir_module: &IrModule) -> Result<Vec<u8>, String> {
        let mut gen = Self::new();
        gen.build_type_section(ir_module)?;
        gen.build_function_section(ir_module)?;
        gen.build_export_section(ir_module)?;
        gen.build_code_section(ir_module)?;
        Ok(gen.module.finish())
    }

    fn new() -> Self {
        Self {
            module: Module::new(),
            func_indices: HashMap::new(),
        }
    }

    fn build_type_section(&mut self, ir_module: &IrModule) -> Result<(), String> {
        let mut types = TypeSection::new();

        for func in &ir_module.functions {
            let mut param_types = Vec::new();
            for (_, param_type) in &func.params {
                param_types.push(param_type.to_wasm_type()?);
            }
            let return_types = vec![func.return_type.to_wasm_type()?];
            types.ty().function(param_types, return_types);
        }

        self.module.section(&types);
        Ok(())
    }

    fn build_function_section(&mut self, ir_module: &IrModule) {
        let mut functions = FunctionSection::new();
        for (i, func) in ir_module.functions.iter().enumerate() {
            functions.function(i as u32);
            self.func_indices.insert(func.name.clone(), i as u32);
        }
        self.module.section(&functions);
    }

    fn build_export_section(&mut self, ir_module: &IrModule) {
        let mut exports = ExportSection::new();
        let mut func_index = 0u32;

        for func in &ir_module.functions {
            if func.is_exported {
                exports.export(&func.name, ExportKind::Func, func_index);
            }
            func_index += 1;
        }

        self.module.section(&exports);
    }

    fn build_code_section(&mut self, ir_module: &IrModule) -> Result<(), String> {
        let mut codes = CodeSection::new();

        for func in &ir_module.functions {
            let locals_map = self.allocate_locals(func);
            let wasm_func = self.translate_function(func, &locals_map)?;
            codes.function(&wasm_func);
        }

        self.module.section(&codes);
        Ok(())
    }

    fn allocate_locals(&self, func: &IrFunction) -> HashMap<String, u32> {
        let mut locals = HashMap::new();

        // Parameters first (indices 0..n)
        for (i, (param_name, _)) in func.params.iter().enumerate() {
            locals.insert(param_name.clone(), i as u32);
        }

        // Collect all registers used
        let mut next_local = func.params.len() as u32;
        for instr in &func.block.instructions {
            match instr {
                IrInstruction::LoadConst { reg, .. } => {
                    if !locals.contains_key(reg) {
                        locals.insert(reg.clone(), next_local);
                        next_local += 1;
                    }
                }
                IrInstruction::Assign { var, .. } => {
                    if !locals.contains_key(&var.0) {
                        locals.insert(var.0.clone(), next_local);
                        next_local += 1;
                    }
                }
                IrInstruction::BinaryOp { left, right, target, .. } => {
                    for reg in [left, right, target] {
                        if !locals.contains_key(reg) {
                            locals.insert(reg.clone(), next_local);
                            next_local += 1;
                        }
                    }
                }
                IrInstruction::Call { args, target, .. } => {
                    for arg in args {
                        if !locals.contains_key(arg) {
                            locals.insert(arg.clone(), next_local);
                            next_local += 1;
                        }
                    }
                    if !locals.contains_key(target) {
                        locals.insert(target.clone(), next_local);
                        next_local += 1;
                    }
                }
                _ => {}
            }
        }

        locals
    }

    fn translate_function(
        &self,
        func: &IrFunction,
        locals_map: &HashMap<String, u32>,
    ) -> Result<Function, String> {
        let mut wasm_func = Function::new([]);
        let mut instr = wasm_func.instructions();

        for ir_instr in &func.block.instructions {
            self.translate_instruction(&mut instr, ir_instr, locals_map)?;
        }

        instr.end();
        Ok(wasm_func)
    }

    fn translate_instruction(
        &self,
        instr: &mut InstructionSink,
        ir_instr: &IrInstruction,
        locals_map: &HashMap<String, u32>,
    ) -> Result<(), String> {
        match ir_instr {
            IrInstruction::LoadConst { reg, value } => {
                if let Expr::Literal(Literal::Integer(n)) = value {
                    instr.i32_const(*n as i32);
                    let local_idx = locals_map.get(reg).unwrap();
                    instr.local_set(*local_idx);
                } else {
                    return Err("Only integer constants supported".to_string());
                }
            }
            IrInstruction::Assign { var, reg } => {
                let local_idx = locals_map.get(&var.0).unwrap();
                let reg_idx = locals_map.get(reg).unwrap();
                instr.local_get(*reg_idx);
                instr.local_set(*local_idx);
            }
            IrInstruction::Call { func_name, args, target } => {
                for arg in args {
                    let arg_idx = locals_map.get(arg).unwrap();
                    instr.local_get(*arg_idx);
                }
                let func_idx = self.func_indices.get(func_name).unwrap();
                instr.call(*func_idx);
                let target_idx = locals_map.get(target).unwrap();
                instr.local_set(*target_idx);
            }
            IrInstruction::Return { reg } => {
                let reg_idx = locals_map.get(reg).unwrap();
                instr.local_get(*reg_idx);
                instr.return_();
            }
            IrInstruction::BinaryOp { op, left, right, target } => {
                let left_idx = locals_map.get(left).unwrap();
                let right_idx = locals_map.get(right).unwrap();
                let target_idx = locals_map.get(target).unwrap();

                instr.local_get(*left_idx);
                instr.local_get(*right_idx);

                match op {
                    BinaryOp::Add => instr.i32_add(),
                    BinaryOp::Sub => instr.i32_sub(),
                    BinaryOp::Mul => instr.i32_mul(),
                    BinaryOp::Div => instr.i32_div_s(),
                }

                instr.local_set(*target_idx);
            }
            IrInstruction::Branch { .. } => {
                return Err("Branch instructions not supported yet".to_string());
            }
            IrInstruction::Panic { .. } => {
                instr.unreachable();
            }
            IrInstruction::Exit { .. } => {
                instr.return_();
            }
        }
        Ok(())
    }
}
```

**Tests**: Add unit tests for WasmGenerator

**Commit**: `feat(codegen): implement Wasm code generation module`

---

### Phase 8: Integration
**File**: `src/lib.rs`

**Changes**:

```rust
pub mod codegen;

pub use codegen::WasmGenerator;

pub fn compile_to_wasm(source: &str) -> Result<Vec<u8>, String> {
    use crate::lexer::tokenizer::Lexer;
    use crate::parser::Parser;
    use crate::analyzer::TypeChecker;
    use crate::ir::generator::generate_ir_all;

    let tokens = Lexer::new(source).tokenize()?;
    let stmts = Parser::new(tokens).parse()?;
    TypeChecker::new().check(&stmts)?;
    let ir_module = generate_ir_all(&stmts)?;
    WasmGenerator::generate(&ir_module)
}
```

**Commit**: `feat(lib): add compile_to_wasm integration function`

---

### Phase 9: Smoke Tests with Wasmtime
**File**: `src/codegen/tests.rs`

**Changes**:

```rust
use super::super::compile_to_wasm;

#[test]
fn test_simple_arithmetic_pipeline() {
    let code = r#"
        export fn main() -> Int =: 10 :: +(5)
    "#;

    let wasm = compile_to_wasm(code).unwrap();

    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &wasm).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let main_func = instance.get_typed_func::<(), i32>(&mut store, "main").unwrap();
    let result = main_func.call(&mut store, ()).unwrap();

    assert_eq!(result, 15);
}

#[test]
fn test_exported_function_with_params() {
    let code = r#"
        export fn double(n: Int) -> Int =: n :: +(n)
    "#;

    let wasm = compile_to_wasm(code).unwrap();
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &wasm).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let double_func = instance.get_typed_func::<i32, i32>(&mut store, "double").unwrap();
    let result = double_func.call(&mut store, 21).unwrap();

    assert_eq!(result, 42);
}

#[test]
fn test_complex_arithmetic() {
    let code = r#"
        export fn calculate() -> Int =: 10 :: *(2) :: +(5)
    "#;

    let wasm = compile_to_wasm(code).unwrap();
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &wasm).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let calc_func = instance.get_typed_func::<(), i32>(&mut store, "calculate").unwrap();
    let result = calc_func.call(&mut store, ()).unwrap();

    assert_eq!(result, 25);
}
```

**Commit**: `test(codegen): add smoke tests with wasmtime`

---

## Workflow

### Branch Strategy
- **Current Branch**: `vk/d21d-wasm` (per AGENTS.md, no new feature branch needed)
- **Base for PR**: `dev`

### Issue Creation
```bash
gh issue create --title "feat: Implement Wasm backend codegen with arithmetic support" \
  --body "Implement Wasm codegen for Iris compiler:
  - Add export flag to FunctionDefinition
  - Add arithmetic operators (+, -, *, /) with native Wasm instruction mapping
  - Implement IrFunction and IrModule types
  - Create WasmGenerator using wasm-encoder
  - Support i32 arithmetic and function calls
  - Export functions marked with \`export fn\`
  - Add smoke tests with wasmtime"
```

### Implementation Order
1. Phase 1 (Lexer) → Commit
2. Phase 2 (AST) → Commit
3. Phase 3 (Parser) → Commit
4. Phase 4 (IR types) → Commit
5. Phase 5 (IR generator) → Commit
6. Phase 6 (Dependencies) → Commit
7. Phase 7 (Codegen module) → Commit
8. Phase 8 (Integration) → Commit
9. Phase 9 (Tests) → Commit

### Quality Gates
- After each phase: `cargo test` (all tests pass)
- After each phase: `cargo clippy` (no warnings)
- After each phase: `cargo fmt --check` (formatted)

### PR Creation
```bash
gh pr create --base dev \
  --title "feat: Implement Wasm backend codegen with arithmetic support" \
  --body "## Summary
  - Add \`is_exported\` flag to \`Stmt::FunctionDefinition\`
  - Create \`IrFunction\` and \`IrModule\` types
  - Implement \`WasmGenerator\` with wasm-encoder
  - Map Type::Int to i32
  - Support LoadConst, Assign, Call, Return, Panic, BinaryOp instructions
  - Export functions with \`export fn\` syntax
  - Add smoke test with wasmtime

  ## Test Results
  - All 29 existing tests pass
  - 3 new smoke tests pass (test_simple_arithmetic_pipeline, test_exported_function_with_params, test_complex_arithmetic)
  - Code formatted with cargo fmt
  - No clippy warnings

  ## Test Cases
  - \`test_simple_arithmetic_pipeline\`: Compiles and executes \`10 :: +(5)\` → returns 15
  - \`test_exported_function_with_params\`: Verifies \`double(21)\` → returns 42
  - \`test_complex_arithmetic\`: Verifies \`10 :: *(2) :: +(5)\` → returns 25"
```

---

## Out of Scope (Future PRs)
- `IrInstruction::Branch` → Wasm blocks/br instructions
- `PipelineStep::Force` / `PipelineStep::ErrorPropagate` → Control flow
- Non-Int types (String, Float, etc.)
- External function imports (FFI)
- Memory management
- Optimizations (register allocation, dead code elimination)

---

## Notes
- **Branch**: Currently on `vk/d21d-wasm` - continue on this branch
- **Dependencies**: No external deps currently, clean slate
- **Breaking Changes**: `Stmt::FunctionDefinition` changes require test updates
- **Linear Code Only**: Branch instructions return error for this PR
