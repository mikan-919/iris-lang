use crate::ast::{Expr, PipelineStep, Stmt};
use crate::ir::{BasicBlock, IrFunction, IrInstruction, IrModule, Variable};

pub fn generate_ir(stmt: &Stmt) -> BasicBlock {
    let mut block = BasicBlock::new();

    if let Stmt::Binding { name, expr } = stmt {
        let (reg, expr_block) = generate_expr_ir(expr);
        block.instructions.extend(expr_block);

        block.add(IrInstruction::Assign {
            var: Variable(name.clone()),
            reg,
        });
    }

    block
}

fn generate_expr_ir(expr: &Expr) -> (String, Vec<IrInstruction>) {
    let mut instructions = Vec::new();

    match expr {
        Expr::Literal(lit) => {
            let reg = format!("t{}", instructions.len());
            instructions.push(IrInstruction::LoadConst {
                reg: reg.clone(),
                value: Expr::Literal(lit.clone()),
            });
            (reg, instructions)
        }
        Expr::Identifier(name) => {
            let reg = format!("t{}", instructions.len());
            instructions.push(IrInstruction::LoadConst {
                reg: reg.clone(),
                value: Expr::Identifier(name.clone()),
            });
            (reg, instructions)
        }
        Expr::BinaryOp { left, op, right } => {
            let (left_reg, left_instrs) = generate_expr_ir(left);
            instructions.extend(left_instrs);
            let (right_reg, right_instrs) = generate_expr_ir(right);
            instructions.extend(right_instrs);
            let result_reg = format!("t{}", instructions.len());
            instructions.push(IrInstruction::BinaryOp {
                op: *op,
                left: left_reg,
                right: right_reg,
                target: result_reg.clone(),
            });
            (result_reg, instructions)
        }
        Expr::Pipeline { initial, steps } => {
            let (mut current_reg, initial_instrs) = generate_expr_ir(initial);
            instructions.extend(initial_instrs);

            for step in steps {
                match step {
                    PipelineStep::FunctionCall {
                        name: func_name,
                        args,
                    } => {
                        let mut call_args = vec![current_reg.clone()];
                        for arg in args {
                            let (arg_reg, arg_instrs) = generate_expr_ir(arg);
                            instructions.extend(arg_instrs);
                            call_args.push(arg_reg);
                        }
                        let result_reg = format!("t{}", instructions.len());
                        instructions.push(IrInstruction::Call {
                            func_name: func_name.clone(),
                            args: call_args,
                            target: result_reg.clone(),
                        });
                        current_reg = result_reg;
                    }
                    PipelineStep::Force => {
                        let branch_true = format!("bb_panic_{}", instructions.len());
                        let branch_false = format!("bb_continue_{}", instructions.len());

                        instructions.push(IrInstruction::Branch {
                            cond: current_reg.clone(),
                            true_block: branch_true,
                            false_block: branch_false,
                        });

                        instructions.push(IrInstruction::Panic {
                            msg: "Force unwrap failed: value is None".to_string(),
                        });
                    }
                    PipelineStep::ErrorPropagate => {
                        let branch_true = format!("bb_return_err_{}", instructions.len());
                        let branch_false = format!("bb_continue_{}", instructions.len());

                        instructions.push(IrInstruction::Branch {
                            cond: current_reg.clone(),
                            true_block: branch_true,
                            false_block: branch_false,
                        });

                        instructions.push(IrInstruction::Return {
                            reg: current_reg.clone(),
                        });
                    }
                    _ => {}
                }
            }

            (current_reg, instructions)
        }
        _ => (String::new(), instructions),
    }
}

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

        instructions.push(IrInstruction::Return { reg: result_reg });

        let block = BasicBlock {
            label: Some("entry".to_string()),
            instructions,
        };

        Ok(IrFunction {
            name: name.clone(),
            is_exported: *is_exported,
            params: params.clone(),
            return_type: return_type.clone(),
            block,
        })
    } else {
        Err("Expected FunctionDefinition".to_string())
    }
}

pub fn generate_ir_all(stmts: &[Stmt]) -> Result<IrModule, String> {
    let mut module = IrModule::new();

    for stmt in stmts {
        if matches!(stmt, Stmt::FunctionDefinition { .. }) {
            module.functions.push(generate_ir_for_function(stmt)?);
        }
    }

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Literal, PipelineStep};

    #[test]
    fn test_generate_ir_for_let_binding() {
        let stmt = Stmt::Binding {
            name: "x".to_string(),
            expr: Expr::Literal(Literal::Integer(10)),
        };

        let block = generate_ir(&stmt);

        assert_eq!(block.instructions.len(), 2);

        if let IrInstruction::LoadConst { reg, value } = &block.instructions[0] {
            assert_eq!(reg, "t0");
            assert!(matches!(value, Expr::Literal(Literal::Integer(10))));
        } else {
            panic!("Expected LoadConst as first instruction");
        }

        if let IrInstruction::Assign { var, reg } = &block.instructions[1] {
            assert_eq!(var.0, "x");
            assert_eq!(reg, "t0");
        } else {
            panic!("Expected Assign as second instruction");
        }
    }

    #[test]
    fn test_pipeline_function_call() {
        let stmt = Stmt::Binding {
            name: "result".to_string(),
            expr: Expr::Pipeline {
                initial: Box::new(Expr::Literal(Literal::Integer(10))),
                steps: vec![PipelineStep::FunctionCall {
                    name: "double".to_string(),
                    args: vec![],
                }],
            },
        };

        let block = generate_ir(&stmt);

        assert!(block.instructions.len() >= 3);

        if let IrInstruction::LoadConst { reg, value } = &block.instructions[0] {
            assert_eq!(reg, "t0");
            assert!(matches!(value, Expr::Literal(Literal::Integer(10))));
        } else {
            panic!("Expected LoadConst for initial value");
        }

        if let IrInstruction::Call {
            func_name,
            args,
            target,
        } = &block.instructions[1]
        {
            assert_eq!(func_name, "double");
            assert_eq!(args, &vec!["t0".to_string()]);
            assert_eq!(target, "t1");
        } else {
            panic!("Expected Call instruction for pipeline step");
        }

        if let IrInstruction::Assign { var, reg } = &block.instructions[2] {
            assert_eq!(var.0, "result");
            assert_eq!(reg, "t1");
        } else {
            panic!("Expected Assign instruction");
        }
    }

    #[test]
    fn test_pipeline_force_operator() {
        let stmt = Stmt::Binding {
            name: "result".to_string(),
            expr: Expr::Pipeline {
                initial: Box::new(Expr::Identifier("maybe_value".to_string())),
                steps: vec![PipelineStep::Force],
            },
        };

        let block = generate_ir(&stmt);

        assert!(block.instructions.len() >= 2);

        let has_branch = block
            .instructions
            .iter()
            .any(|instr| matches!(instr, IrInstruction::Branch { .. }));
        let has_panic = block
            .instructions
            .iter()
            .any(|instr| matches!(instr, IrInstruction::Panic { .. }));

        assert!(has_branch, "Expected Branch instruction for Force operator");
        assert!(has_panic, "Expected Panic instruction for Force operator");
    }

    #[test]
    fn test_pipeline_try_operator() {
        let stmt = Stmt::Binding {
            name: "result".to_string(),
            expr: Expr::Pipeline {
                initial: Box::new(Expr::Identifier("maybe_result".to_string())),
                steps: vec![PipelineStep::ErrorPropagate],
            },
        };

        let block = generate_ir(&stmt);

        assert!(!block.instructions.is_empty());

        let has_branch = block
            .instructions
            .iter()
            .any(|instr| matches!(instr, IrInstruction::Branch { .. }));
        let has_return = block
            .instructions
            .iter()
            .any(|instr| matches!(instr, IrInstruction::Return { .. }));

        assert!(has_branch, "Expected Branch instruction for Try operator");
        assert!(has_return, "Expected Return instruction for Try operator");
    }

    #[test]
    fn test_complex_pipeline_with_multiple_steps() {
        let stmt = Stmt::Binding {
            name: "result".to_string(),
            expr: Expr::Pipeline {
                initial: Box::new(Expr::Literal(Literal::Integer(10))),
                steps: vec![
                    PipelineStep::FunctionCall {
                        name: "double".to_string(),
                        args: vec![],
                    },
                    PipelineStep::FunctionCall {
                        name: "increment".to_string(),
                        args: vec![],
                    },
                ],
            },
        };

        let block = generate_ir(&stmt);

        assert!(block.instructions.len() >= 4);

        let call_count = block
            .instructions
            .iter()
            .filter(|instr| matches!(instr, IrInstruction::Call { .. }))
            .count();

        assert_eq!(
            call_count, 2,
            "Expected 2 Call instructions for 2 pipeline steps"
        );
    }

    #[test]
    fn test_generate_ir_for_function_simple() {
        let stmt = Stmt::FunctionDefinition {
            name: "add".to_string(),
            is_exported: false,
            params: vec![
                ("a".to_string(), crate::ast::Type::Simple("Int".to_string())),
                ("b".to_string(), crate::ast::Type::Simple("Int".to_string())),
            ],
            return_type: crate::ast::Type::Simple("Int".to_string()),
            body: Box::new(Expr::Literal(Literal::Integer(42))),
        };

        let ir_func = generate_ir_for_function(&stmt).unwrap();

        assert_eq!(ir_func.name, "add");
        assert!(!ir_func.is_exported);
        assert_eq!(ir_func.params.len(), 2);
        assert_eq!(ir_func.block.label, Some("entry".to_string()));
        assert!(ir_func.block.instructions.len() >= 2);
        assert!(matches!(
            ir_func.block.instructions.last(),
            Some(IrInstruction::Return { .. })
        ));
    }

    #[test]
    fn test_generate_ir_for_function_with_pipeline() {
        let stmt = Stmt::FunctionDefinition {
            name: "greet".to_string(),
            is_exported: false,
            params: vec![(
                "name".to_string(),
                crate::ast::Type::Simple("String".to_string()),
            )],
            return_type: crate::ast::Type::Simple("String".to_string()),
            body: Box::new(Expr::Pipeline {
                initial: Box::new(Expr::Literal(Literal::String("Hello".to_string()))),
                steps: vec![PipelineStep::FunctionCall {
                    name: "concat".to_string(),
                    args: vec![Expr::Identifier("name".to_string())],
                }],
            }),
        };

        let ir_func = generate_ir_for_function(&stmt).unwrap();

        assert_eq!(ir_func.name, "greet");
        assert!(ir_func.block.instructions.len() >= 3);
        assert!(matches!(
            ir_func.block.instructions.last(),
            Some(IrInstruction::Return { .. })
        ));
    }
}
