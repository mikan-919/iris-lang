use crate::ast::{BinaryOp, Expr, Literal, Type};
use crate::ir::{BasicBlock, IrFunction, IrInstruction, IrModule};
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, Module, TypeSection, ValType,
};

#[derive(Debug, Clone)]
pub struct WasmGenerator {
    type_indices: std::collections::HashMap<String, u32>,
}

impl WasmGenerator {
    pub fn new() -> Self {
        Self {
            type_indices: std::collections::HashMap::new(),
        }
    }

    pub fn compile(&mut self, ir_module: &IrModule) -> Result<Vec<u8>, String> {
        let mut module = Module::new();

        let mut types = TypeSection::new();
        for func in &ir_module.functions {
            let type_idx = self.encode_function_type(&mut types, func)?;
            self.type_indices.insert(func.name.clone(), type_idx);
        }
        module.section(&types);

        let mut imports = ImportSection::new();
        let mut func_index: u32 = 0;
        for func in &ir_module.functions {
            if func.is_external {
                if let Some(external_name) = &func.external_name {
                    imports.import("env", external_name, EntityType::Function(func_index));
                    self.type_indices.insert(func.name.clone(), func_index);
                    func_index += 1;
                }
            } else {
                self.type_indices.insert(func.name.clone(), func_index);
                func_index += 1;
            }
        }
        module.section(&imports);

        let mut functions = FunctionSection::new();
        for func in &ir_module.functions {
            if !func.is_external {
                let type_idx = *self
                    .type_indices
                    .get(&func.name)
                    .ok_or_else(|| format!("Type index not found for function: {}", func.name))?;
                functions.function(type_idx);
            }
        }
        module.section(&functions);

        let mut exports = ExportSection::new();
        for func in ir_module.functions.iter() {
            if func.is_exported {
                let func_idx = *self
                    .type_indices
                    .get(&func.name)
                    .ok_or_else(|| format!("Type index not found for function: {}", func.name))?;
                exports.export(&func.name, ExportKind::Func, func_idx);
            }
        }
        module.section(&exports);

        let mut codes = CodeSection::new();
        for func in &ir_module.functions {
            if !func.is_external {
                self.encode_function_body(&mut codes, func)?;
            }
        }
        module.section(&codes);

        Ok(module.finish())
    }

    fn encode_function_type(
        &mut self,
        types: &mut TypeSection,
        func: &IrFunction,
    ) -> Result<u32, String> {
        let mut param_types: Vec<ValType> = Vec::new();
        for (_name, ty) in &func.params {
            param_types.push(self.type_to_valtype(ty)?);
        }

        let result_type = self.type_to_valtype(&func.return_type)?;
        let results = vec![result_type];

        let type_idx = types.len();
        types.function(param_types, results);
        Ok(type_idx)
    }

    fn type_to_valtype(&self, ty: &Type) -> Result<ValType, String> {
        match ty {
            Type::Int => Ok(ValType::I32),
            Type::Float => Ok(ValType::F64),
            Type::Bool => Ok(ValType::I32),
            Type::Unit => Ok(ValType::I32),
            _ => Err(format!("Type {:?} not yet supported in Wasm", ty)),
        }
    }

    fn encode_function_body(
        &self,
        codes: &mut CodeSection,
        func: &IrFunction,
    ) -> Result<(), String> {
        // Collect all temporary registers used in the function
        let mut temp_regs = std::collections::HashSet::new();
        for instr in &func.block.instructions {
            match instr {
                IrInstruction::LoadConst { reg, .. } => {
                    temp_regs.insert(reg.clone());
                }
                IrInstruction::LoadLocal { reg, .. } => {
                    temp_regs.insert(reg.clone());
                }
                IrInstruction::BinaryOp {
                    left,
                    right,
                    target,
                    ..
                } => {
                    temp_regs.insert(left.clone());
                    temp_regs.insert(right.clone());
                    temp_regs.insert(target.clone());
                }
                IrInstruction::Call { args, target, .. } => {
                    for arg in args {
                        temp_regs.insert(arg.clone());
                    }
                    temp_regs.insert(target.clone());
                }
                IrInstruction::Assign { var: _, reg } => {
                    temp_regs.insert(reg.clone());
                }
                IrInstruction::Return { reg } => {
                    temp_regs.insert(reg.clone());
                }
                IrInstruction::Branch { cond, .. } => {
                    temp_regs.insert(cond.clone());
                }
                IrInstruction::Exit { value } => {
                    temp_regs.insert(value.clone());
                }
                IrInstruction::Panic { .. } => {}
            }
        }

        // Build locals: first parameters, then temporary registers
        let mut all_locals: Vec<(String, u32)> = Vec::new();
        for (param_idx, (param_name, _)) in func.params.iter().enumerate() {
            all_locals.push((param_name.clone(), param_idx as u32));
        }

        let temp_reg_type = self.type_to_valtype(&func.return_type)?;
        for reg in temp_regs.iter() {
            if !all_locals.iter().any(|(name, _)| name == reg) {
                let idx = all_locals.len() as u32;
                all_locals.push((reg.clone(), idx));
            }
        }

        // Create locals for Wasm function (additional locals only, not parameters)
        // Function::new() expects Vec<(count, type)> where count is the number of consecutive locals
        let additional_locals: Vec<(u32, ValType)> = {
            let temp_reg_count = temp_regs.len() as u32;
            if temp_reg_count > 0 {
                vec![(temp_reg_count, temp_reg_type)]
            } else {
                vec![]
            }
        };

        let locals_map: std::collections::HashMap<String, u32> = all_locals.into_iter().collect();

        let mut wasm_func = Function::new(additional_locals);

        self.encode_basic_block(&mut wasm_func, &func.block, &locals_map, &self.type_indices)?;

        wasm_func.instruction(&Instruction::End);

        codes.function(&wasm_func);
        Ok(())
    }

    fn encode_basic_block(
        &self,
        wasm_func: &mut Function,
        block: &BasicBlock,
        locals_map: &std::collections::HashMap<String, u32>,
        func_indices: &std::collections::HashMap<String, u32>,
    ) -> Result<(), String> {
        for instr in &block.instructions {
            self.encode_instruction(wasm_func, instr, locals_map, func_indices)?;
        }
        Ok(())
    }

    fn encode_instruction(
        &self,
        wasm_func: &mut Function,
        instr: &IrInstruction,
        locals_map: &std::collections::HashMap<String, u32>,
        func_indices: &std::collections::HashMap<String, u32>,
    ) -> Result<(), String> {
        match instr {
            IrInstruction::LoadConst { reg, value } => {
                let local_idx = locals_map
                    .get(reg)
                    .ok_or_else(|| format!("Register not found: {}", reg))?;
                self.encode_expr(wasm_func, value)?;
                wasm_func.instruction(&Instruction::LocalSet(*local_idx));
            }
            IrInstruction::LoadLocal { reg, name } => {
                let name_idx = locals_map
                    .get(name)
                    .ok_or_else(|| format!("Local variable not found: {}", name))?;
                let reg_idx = locals_map
                    .get(reg)
                    .ok_or_else(|| format!("Register not found: {}", reg))?;
                wasm_func.instruction(&Instruction::LocalGet(*name_idx));
                wasm_func.instruction(&Instruction::LocalSet(*reg_idx));
            }
            IrInstruction::Assign { var, reg } => {
                let var_idx = locals_map
                    .get(&var.0)
                    .ok_or_else(|| format!("Variable not found: {}", var.0))?;
                let reg_idx = locals_map
                    .get(reg)
                    .ok_or_else(|| format!("Register not found: {}", reg))?;
                wasm_func.instruction(&Instruction::LocalGet(*reg_idx));
                wasm_func.instruction(&Instruction::LocalSet(*var_idx));
            }
            IrInstruction::Return { reg } => {
                let local_idx = locals_map
                    .get(reg)
                    .ok_or_else(|| format!("Register not found: {}", reg))?;
                wasm_func.instruction(&Instruction::LocalGet(*local_idx));
            }
            IrInstruction::Call {
                func_name,
                args,
                target,
            } => {
                let func_idx = func_indices
                    .get(func_name)
                    .ok_or_else(|| format!("Function not found: {}", func_name))?;

                for arg in args {
                    let local_idx = locals_map
                        .get(arg)
                        .ok_or_else(|| format!("Argument register not found: {}", arg))?;
                    wasm_func.instruction(&Instruction::LocalGet(*local_idx));
                }

                wasm_func.instruction(&Instruction::Call(*func_idx));

                let target_idx = locals_map
                    .get(target)
                    .ok_or_else(|| format!("Target register not found: {}", target))?;
                wasm_func.instruction(&Instruction::LocalSet(*target_idx));
            }
            IrInstruction::Branch {
                cond,
                true_block: _,
                false_block: _,
            } => {
                let cond_idx = locals_map
                    .get(cond)
                    .ok_or_else(|| format!("Condition register not found: {}", cond))?;
                wasm_func.instruction(&Instruction::LocalGet(*cond_idx));
                wasm_func.instruction(&Instruction::Unreachable);
            }
            IrInstruction::Panic { msg: _ } => {
                wasm_func.instruction(&Instruction::Unreachable);
            }
            IrInstruction::Exit { value } => {
                let val_idx = locals_map
                    .get(value)
                    .ok_or_else(|| format!("Value register not found: {}", value))?;
                wasm_func.instruction(&Instruction::LocalGet(*val_idx));
            }
            IrInstruction::BinaryOp {
                op,
                left,
                right,
                target,
            } => {
                let left_idx = locals_map
                    .get(left)
                    .ok_or_else(|| format!("Left operand not found: {}", left))?;
                let right_idx = locals_map
                    .get(right)
                    .ok_or_else(|| format!("Right operand not found: {}", right))?;
                let target_idx = locals_map
                    .get(target)
                    .ok_or_else(|| format!("Target register not found: {}", target))?;

                wasm_func.instruction(&Instruction::LocalGet(*left_idx));
                wasm_func.instruction(&Instruction::LocalGet(*right_idx));

                match op {
                    BinaryOp::Add => wasm_func.instruction(&Instruction::I32Add),
                    BinaryOp::Sub => wasm_func.instruction(&Instruction::I32Sub),
                    BinaryOp::Mul => wasm_func.instruction(&Instruction::I32Mul),
                    BinaryOp::Div => wasm_func.instruction(&Instruction::I32DivS),
                };
                wasm_func.instruction(&Instruction::LocalSet(*target_idx));
            }
        }
        Ok(())
    }

    fn encode_expr(&self, wasm_func: &mut Function, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::Integer(i) => {
                    wasm_func.instruction(&Instruction::I32Const(*i as i32));
                }
                Literal::String(_) => {
                    return Err("String literals not yet supported in Wasm codegen".to_string());
                }
            },
            Expr::Identifier(name) => {
                return Err(format!("Unexpected identifier expression: {}", name));
            }
            Expr::FunctionCall { name: _, args: _ } => {
                return Err("Function calls in expressions not yet supported".to_string());
            }
            _ => {
                return Err("Complex expressions not yet supported in Wasm codegen".to_string());
            }
        }
        Ok(())
    }
}

impl Default for WasmGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_to_valtype() {
        let generator = WasmGenerator::new();

        assert_eq!(generator.type_to_valtype(&Type::Int), Ok(ValType::I32));
        assert_eq!(generator.type_to_valtype(&Type::Float), Ok(ValType::F64));
        assert_eq!(generator.type_to_valtype(&Type::Bool), Ok(ValType::I32));
        assert!(generator.type_to_valtype(&Type::String).is_err());
    }

    #[test]
    fn test_simple_add_function() {
        let mut module = IrModule::new();

        let mut block = BasicBlock::new();
        block.add(IrInstruction::BinaryOp {
            op: BinaryOp::Add,
            left: "a".to_string(),
            right: "b".to_string(),
            target: "a".to_string(),
        });
        block.add(IrInstruction::Return {
            reg: "a".to_string(),
        });

        let func = IrFunction {
            name: "add".to_string(),
            is_exported: true,
            is_external: false,
            external_name: None,
            params: vec![("a".to_string(), Type::Int), ("b".to_string(), Type::Int)],
            return_type: Type::Int,
            block,
        };

        module.functions.push(func);

        let mut generator = WasmGenerator::new();
        let wasm_bytes = generator
            .compile(&module)
            .expect("Failed to compile to Wasm");

        assert!(!wasm_bytes.is_empty());

        assert_eq!(&wasm_bytes[0..4], b"\x00\x61\x73\x6d");
        assert_eq!(&wasm_bytes[4..8], b"\x01\x00\x00\x00");
    }

    #[test]
    fn test_simple_const_return() {
        let mut module = IrModule::new();

        let mut block = BasicBlock::new();
        block.add(IrInstruction::LoadConst {
            reg: "a".to_string(),
            value: Expr::Literal(Literal::Integer(42)),
        });

        let func = IrFunction {
            name: "add".to_string(),
            is_exported: true,
            is_external: false,
            external_name: None,
            params: vec![("a".to_string(), Type::Int), ("b".to_string(), Type::Int)],
            return_type: Type::Int,
            block,
        };

        module.functions.push(func);

        let mut generator = WasmGenerator::new();
        let wasm_bytes = generator
            .compile(&module)
            .expect("Failed to compile to Wasm");

        assert!(!wasm_bytes.is_empty());
        assert_eq!(&wasm_bytes[0..4], b"\x00\x61\x73\x6d");
    }

    #[test]
    fn test_unsupported_type_returns_error() {
        let generator = WasmGenerator::new();

        assert!(generator.type_to_valtype(&Type::String).is_err());
        assert!(generator.type_to_valtype(&Type::Any).is_err());
    }

    #[test]
    fn test_empty_module() {
        let module = IrModule::new();

        let mut generator = WasmGenerator::new();
        let wasm_bytes = generator
            .compile(&module)
            .expect("Failed to compile to Wasm");

        assert!(!wasm_bytes.is_empty());
        assert_eq!(&wasm_bytes[0..4], b"\x00\x61\x73\x6d");
    }

    #[test]
    fn test_multiple_functions() {
        let mut module = IrModule::new();

        let mut block1 = BasicBlock::new();
        block1.add(IrInstruction::Return {
            reg: "a".to_string(),
        });

        let func1 = IrFunction {
            name: "identity".to_string(),
            is_exported: true,
            is_external: false,
            external_name: None,
            params: vec![("a".to_string(), Type::Int)],
            return_type: Type::Int,
            block: block1,
        };

        let mut block2 = BasicBlock::new();
        block2.add(IrInstruction::Return {
            reg: "x".to_string(),
        });

        let func2 = IrFunction {
            name: "identity2".to_string(),
            is_exported: true,
            is_external: false,
            external_name: None,
            params: vec![("x".to_string(), Type::Int)],
            return_type: Type::Int,
            block: block2,
        };

        module.functions.push(func1);
        module.functions.push(func2);

        let mut generator = WasmGenerator::new();
        let wasm_bytes = generator
            .compile(&module)
            .expect("Failed to compile to Wasm");

        assert!(!wasm_bytes.is_empty());
        assert_eq!(&wasm_bytes[0..4], b"\x00\x61\x73\x6d");
    }

    #[test]
    fn test_wasm_magic_number() {
        use crate::ir;

        let mut module = ir::IrModule::new();

        let mut block = BasicBlock::new();
        block.add(IrInstruction::BinaryOp {
            op: BinaryOp::Add,
            left: "a".to_string(),
            right: "b".to_string(),
            target: "a".to_string(),
        });
        block.add(IrInstruction::Return {
            reg: "a".to_string(),
        });

        let func = IrFunction {
            name: "add".to_string(),
            is_exported: true,
            is_external: false,
            external_name: None,
            params: vec![("a".to_string(), Type::Int), ("b".to_string(), Type::Int)],
            return_type: Type::Int,
            block,
        };

        module.functions.push(func);

        let mut generator = WasmGenerator::new();
        let wasm_bytes = generator
            .compile(&module)
            .expect("Failed to compile to Wasm");

        assert!(!wasm_bytes.is_empty());
        assert!(wasm_bytes.len() > 10);

        let magic = &wasm_bytes[0..4];
        assert_eq!(magic, b"\x00\x61\x73\x6d");

        let version = &wasm_bytes[4..8];
        assert_eq!(version, b"\x01\x00\x00\x00");
    }
}
