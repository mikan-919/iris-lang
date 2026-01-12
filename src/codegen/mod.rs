use wasm_encoder::*;
use crate::ast::{BinaryOp, Type};
use crate::ir::{IrModule, IrFunction, IrInstruction};

pub struct WasmGenerator {
    module: Module,
    func_indices: std::collections::HashMap<String, u32>,
}

impl WasmGenerator {
    pub fn generate(ir_module: &IrModule) -> Result<Vec<u8>, String> {
        let mut gen = Self::new();
        gen.build_type_section(ir_module)?;
        gen.build_function_section(ir_module);
        gen.build_export_section(ir_module);
        gen.build_code_section(ir_module)?;
        Ok(gen.module.finish())
    }

    fn new() -> Self {
        Self {
            module: Module::new(),
            func_indices: std::collections::HashMap::new(),
        }
    }

    fn build_type_section(&mut self, ir_module: &IrModule) -> Result<(), String> {
        let mut types = TypeSection::new();

        for func in &ir_module.functions {
            let mut param_types = Vec::new();
            for (_, param_type) in &func.params {
                param_types.push(param_type.to_wasm_type().unwrap_or("i32").into());
            }
            let return_types = vec![func.return_type.to_wasm_type().unwrap_or("i32").into()];
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

    fn allocate_locals(&self, func: &IrFunction) -> std::collections::HashMap<String, u32> {
        let mut locals = std::collections::HashMap::new();

        for (i, (param_name, _)) in func.params.iter().enumerate() {
            locals.insert(param_name.clone(), i as u32);
        }

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
                IrInstruction::Return { reg } => {
                    let reg_idx = locals.get(reg).unwrap();
                    // Just get the register value, no set
                }
                IrInstruction::BinaryOp { left, right, target, .. } => {
                    for r in [left, right, target] {
                        if !locals.contains_key(r) {
                            locals.insert(r.clone(), next_local);
                            next_local += 1;
                        }
                    }
                    // Load left, load right, do op, store result
                    let left_idx = locals.get(left).unwrap();
                    let right_idx = locals.get(right).unwrap();
                    let target_idx = locals.get(target).unwrap();

                    // left, right, op, target_idx
                }
                IrInstruction::Branch { .. } => {
                    return Err("Branch instructions not supported yet".to_string());
                }
                IrInstruction::Panic { .. } => {
                    unreachable();
                }
                IrInstruction::Exit { reg } => {
                    let reg_idx = locals.get(reg).unwrap();
                    // Return the register value
                }
            }
        }

        locals
    }

    fn translate_function(&self, func: &IrFunction, locals_map: &std::collections::HashMap<String, u32>) -> Result<Function, String> {
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
        instr_sink: &mut InstructionSink,
        ir_instr: &IrInstruction,
        locals_map: &std::collections::HashMap<String, u32>,
    ) -> Result<(), String> {
        match ir_instr {
            IrInstruction::LoadConst { reg, value } => {
                if let crate::ast::Expr::Literal(crate::ast::Literal::Integer(n)) = value {
                    instr_sink.i32_const(*n as i32);
                    let local_idx = locals_map.get(reg).unwrap();
                    instr_sink.local_set(*local_idx);
                } else {
                    return Err("Only integer constants supported".to_string());
                }
            }
            IrInstruction::Assign { var, reg } => {
                let local_idx = locals_map.get(&var.0).unwrap();
                let reg_idx = locals_map.get(reg).unwrap();
                instr_sink.local_get(*reg_idx);
                instr_sink.local_set(*local_idx);
            }
            IrInstruction::Call { func_name, args, target } => {
                for arg in args {
                    let arg_idx = locals_map.get(arg).unwrap();
                    instr_sink.local_get(*arg_idx);
                }
                let func_idx = self.func_indices.get(func_name)
                    .ok_or_else(|| format!("Undefined function '{}'", func_name))?;
                instr_sink.call(*func_idx);
                let target_idx = locals_map.get(target).unwrap();
                instr_sink.local_set(*target_idx);
            }
            IrInstruction::Return { reg } => {
                let reg_idx = locals.get(reg).unwrap();
                instr_sink.local_get(*reg_idx);
                instr_sink.return_();
            }
            IrInstruction::BinaryOp { op, left, right, target } => {
                let left_idx = locals.get(left).unwrap();
                let right_idx = locals.get(right).unwrap();
                let target_idx = locals.get(target).unwrap();

                instr_sink.local_get(*left_idx);
                instr_sink.local_get(*right_idx);

                match op {
                    BinaryOp::Add => instr_sink.i32_add(),
                    BinaryOp::Sub => instr_sink.i32_sub(),
                    BinaryOp::Mul => instr_sink.i32_mul(),
                    BinaryOp::Div => instr_sink.i32_div_s(),
                }

                instr_sink.local_set(*target_idx);
            }
            IrInstruction::Branch { .. } => {
                return Err("Branch instructions not supported yet".to_string());
            }
            IrInstruction::Panic { .. } => {
                unreachable();
            }
            IrInstruction::Exit { reg } => {
                let reg_idx = locals.get(reg).unwrap();
                instr_sink.local_get(*reg_idx);
                instr_sink.return_();
            }
        }
        Ok(())
    }
}


impl WasmGenerator {
    pub fn generate(ir_module: &IrModule) -> Result<Vec<u8>, String> {
        let mut gen = Self::new();
        gen.build_type_section(ir_module)?;
        gen.build_function_section(ir_module);
        gen.build_export_section(ir_module);
        gen.build_code_section(ir_module)?;
        Ok(gen.module.finish())
    }

    fn new() -> Self {
        Self {
            module: Module::new(),
            func_indices: std::collections::HashMap::new(),
        }
    }

    fn build_type_section(&mut self, ir_module: &IrModule) -> Result<(), String> {
        let mut types = TypeSection::new();

        for func in &ir_module.functions {
            let mut param_types = Vec::new();
            for (_, param_type) in &func.params {
                param_types.push(param_type.to_wasm_type().unwrap_or("i32").into());
            }
            let return_types = vec![func.return_type.to_wasm_type().unwrap_or("i32").into()];
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

    fn allocate_locals(&self, func: &IrFunction) -> std::collections::HashMap<String, u32> {
        let mut locals = std::collections::HashMap::new();

        for (i, (param_name, _)) in func.params.iter().enumerate() {
            locals.insert(param_name.clone(), i as u32);
        }

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
                IrInstruction::BinaryOp { left, right, target, .. } => {
                    for reg in [left, right, target] {
                        if !locals.contains_key(reg) {
                            locals.insert(reg.clone(), next_local);
                            next_local += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        locals
    }

    fn translate_function(&self, func: &IrFunction, locals_map: &std::collections::HashMap<String, u32>) -> Result<Function, String> {
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
        instr_sink: &mut InstructionSink,
        ir_instr: &IrInstruction,
        locals_map: &std::collections::HashMap<String, u32>,
    ) -> Result<(), String> {
        match ir_instr {
            IrInstruction::LoadConst { reg, value } => {
                if let crate::ast::Expr::Literal(crate::ast::Literal::Integer(n)) = value {
                    instr_sink.i32_const(*n as i32);
                    let local_idx = locals_map.get(reg).unwrap();
                    instr_sink.local_set(*local_idx);
                } else {
                    return Err("Only integer constants supported".to_string());
                }
            }
            IrInstruction::Assign { var, reg } => {
                let local_idx = locals_map.get(&var.0).unwrap();
                let reg_idx = locals_map.get(reg).unwrap();
                instr_sink.local_get(*reg_idx);
                instr_sink.local_set(*local_idx);
            }
            IrInstruction::Call { func_name, args, target } => {
                for arg in args {
                    let arg_idx = locals_map.get(arg).unwrap();
                    instr_sink.local_get(*arg_idx);
                }
                let func_idx = self.func_indices.get(func_name)
                    .ok_or_else(|| format!("Undefined function '{}'", func_name))?;
                instr_sink.call(*func_idx);
                let target_idx = locals_map.get(target).unwrap();
                instr_sink.local_set(*target_idx);
            }
            IrInstruction::Return { reg } => {
                let reg_idx = locals_map.get(reg).unwrap();
                instr_sink.local_get(*reg_idx);
                instr_sink.return_();
            }
            IrInstruction::BinaryOp { op, left, right, target } => {
                let left_idx = locals_map.get(left).unwrap();
                let right_idx = locals_map.get(right).unwrap();
                let target_idx = locals_map.get(target).unwrap();

                instr_sink.local_get(*left_idx);
                instr_sink.local_get(*right_idx);

                match op {
                    BinaryOp::Add => instr_sink.i32_add(),
                    BinaryOp::Sub => instr_sink.i32_sub(),
                    BinaryOp::Mul => instr_sink.i32_mul(),
                    BinaryOp::Div => instr_sink.i32_div_s(),
                }

                instr_sink.local_set(*target_idx);
            }
            IrInstruction::Branch { .. } => {
                return Err("Branch instructions not supported yet".to_string());
            }
            IrInstruction::Panic { .. } => {
                instr_sink.unreachable();
            }
            IrInstruction::Exit { reg } => {
                let reg_idx = locals_map.get(reg).unwrap();
                instr_sink.local_get(*reg_idx);
                instr_sink.return_();
            }
        }
        Ok(())
    }
}
