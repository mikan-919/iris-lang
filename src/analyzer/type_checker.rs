use crate::analyzer::symbol::SymbolTable;
use crate::ast::*;
use std::collections::HashMap;

fn types_compatible(input: &Type, expected: &Type) -> bool {
    match (input, expected) {
        (Type::Any, _) | (_, Type::Any) => true,
        _ => input == expected,
    }
}

pub struct TypeChecker {
    symbol_table: SymbolTable,
    function_signatures: HashMap<String, Type>,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            function_signatures: HashMap::new(),
        }
    }

    pub fn check(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for stmt in stmts {
            if let Stmt::FunctionDefinition {
                name,
                params,
                return_type,
                ..
            } = stmt
            {
                let func_type = Type::Function {
                    params: params.iter().map(|(_, t)| t.clone()).collect(),
                    return_type: Box::new(return_type.clone()),
                };
                self.function_signatures
                    .insert(name.clone(), func_type.clone());
                self.symbol_table.define(name.clone(), func_type, true);
            }
        }

        for stmt in stmts {
            self.check_stmt(stmt)?;
        }

        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Binding { name, expr } => {
                let ty = self.infer_expr(expr)?;
                self.symbol_table.define(name.clone(), ty, false);
                Ok(())
            }
            Stmt::FunctionDefinition {
                name,
                is_exported: _,
                params,
                return_type,
                body,
            } => {
                self.symbol_table.push_scope();

                for (param_name, param_type) in params {
                    self.symbol_table
                        .define(param_name.clone(), param_type.clone(), false);
                }

                let body_ty = self.infer_expr(body)?;
                let normalized_return_type = return_type.normalize();
                let normalized_body_ty = body_ty.normalize();
                if !types_compatible(&normalized_body_ty, &normalized_return_type) {
                    return Err(format!(
                        "Function '{}' return type mismatch: expected {}, got {}",
                        name, normalized_return_type, normalized_body_ty
                    ));
                }

                let body_ty = self.infer_expr(body)?;
                if body_ty != *return_type {
                    return Err(format!(
                        "Function '{}' return type mismatch: expected {}, got {}",
                        name, return_type, body_ty
                    ));
                }

                self.symbol_table.pop_scope();
                Ok(())
            }
            Stmt::MatchStatement(_) => {
                Err("Match statement type checking not yet implemented".to_string())
            }
            Stmt::ForLoopStatement(_) => {
                Err("For loop statement type checking not yet implemented".to_string())
            }
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> Result<Type, String> {
        match expr {
            Expr::Literal(Literal::Integer(_)) => Ok(Type::Int),
            Expr::Literal(Literal::String(_)) => Ok(Type::String),
            Expr::Identifier(name) => self
                .symbol_table
                .get(name)
                .map(|s| s.ty.clone())
                .ok_or_else(|| format!("Undefined variable '{}'", name)),
            Expr::FunctionCall { name, args } => {
                let func_ty = self
                    .function_signatures
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("Undefined function '{}'", name))?;

                let (param_types, return_type) = match func_ty {
                    Type::Function {
                        params,
                        return_type,
                    } => (params, *return_type),
                    _ => return Err(format!("'{}' is not a function", name))?,
                };

                if args.len() != param_types.len() {
                    return Err(format!(
                        "Function '{}' expects {} arguments, got {}",
                        name,
                        param_types.len(),
                        args.len()
                    ));
                }

                for (arg, expected_ty) in args.iter().zip(&param_types) {
                    let arg_ty = self.infer_expr(arg)?;
                    let normalized_arg_ty = arg_ty.normalize();
                    let normalized_expected_ty = expected_ty.normalize();
                    if !types_compatible(&normalized_arg_ty, &normalized_expected_ty) {
                        return Err(format!(
                            "Argument type mismatch for '{}': expected {}, got {}",
                            name, normalized_expected_ty, normalized_arg_ty
                        ));
                    }
                }

                Ok(return_type)
            }
            Expr::Pipeline { initial, steps } => self.infer_pipeline(initial, steps),
            Expr::Match { .. } => {
                Err("Match expression type checking not yet implemented".to_string())
            }
            Expr::ForLoop(_) => {
                Err("For loop expression type checking not yet implemented".to_string())
            }
        }
    }

    fn infer_pipeline(&mut self, initial: &Expr, steps: &[PipelineStep]) -> Result<Type, String> {
        let mut current_type = self.infer_expr(initial)?;

        for step in steps {
            current_type = self.infer_pipeline_step(&current_type, step)?;
        }

        Ok(current_type)
    }

    fn infer_pipeline_step(
        &mut self,
        input_type: &Type,
        step: &PipelineStep,
    ) -> Result<Type, String> {
        match step {
            PipelineStep::FunctionCall(func_name) => {
                let func_ty = self
                    .function_signatures
                    .get(func_name)
                    .cloned()
                    .ok_or_else(|| format!("Undefined function '{}'", func_name))?;

                match func_ty {
                    Type::Function {
                        params,
                        return_type,
                    } => {
                        if params.len() != 1 {
                            return Err(format!(
                                "Pipeline function '{}' must take exactly 1 argument, got {}",
                                func_name,
                                params.len()
                            ));
                        }

                        let normalized_input = input_type.normalize();
                        let normalized_param = params[0].normalize();
                        if !types_compatible(&normalized_input, &normalized_param) {
                            return Err(format!(
                                "Pipeline type mismatch for '{}': expected {}, got {}",
                                func_name, normalized_param, normalized_input
                            ));
                        }

                        Ok(*return_type)
                    }
                    _ => Err(format!("'{}' is not a function", func_name)),
                }
            }

            PipelineStep::AsyncCall(func_name) => {
                let inner_type = self.infer_pipeline_step(
                    input_type,
                    &PipelineStep::FunctionCall(func_name.clone()),
                )?;
                Ok(Type::future(inner_type))
            }

            PipelineStep::ErrorPropagate => Ok(Type::result(input_type.clone(), Type::String)),

            PipelineStep::Force => match input_type {
                Type::Generic { name, args } if name == "Option" || name == "Result" => {
                    Ok(args[0].clone())
                }
                _ => Err(format!(
                    "Cannot force unwrap non-Option/Result type {}",
                    input_type
                )),
            },

            PipelineStep::ErrorRescue(fallback) => {
                let fallback_type = self.infer_expr(fallback)?;
                let normalized_fallback = fallback_type.normalize();
                match input_type {
                    Type::Generic { name, args } if name == "Result" && args.len() == 2 => {
                        let normalized_ok = args[0].normalize();
                        let normalized_err = args[1].normalize();
                        if !types_compatible(&normalized_fallback, &normalized_ok)
                            && !types_compatible(&normalized_fallback, &normalized_err)
                        {
                            return Err(format!(
                                "Error rescue fallback must match Ok type {} or Err type {}, got {}",
                                normalized_ok, normalized_err, normalized_fallback
                            ));
                        }
                        Ok(args[0].clone())
                    }
                    _ => Err(format!(
                        "Cannot use error rescue on non-Result type {}",
                        input_type
                    )),
                }
            }

            PipelineStep::Fallback(default_expr) => {
                let default_type = self.infer_expr(default_expr)?;
                let normalized_default = default_type.normalize();
                match input_type {
                    Type::Generic { name, args } if name == "Option" && args.len() == 1 => {
                        let normalized_inner = args[0].normalize();
                        if !types_compatible(&normalized_default, &normalized_inner) {
                            return Err(format!(
                                "Fallback type mismatch: expected {}, got {}",
                                normalized_inner, normalized_default
                            ));
                        }
                        Ok(args[0].clone())
                    }
                    _ => Err(format!(
                        "Cannot use fallback on non-Option type {}",
                        input_type
                    )),
                }
            }

            PipelineStep::BorrowReference(_ref_name) => Ok(input_type.clone()),

            PipelineStep::TupleMerge(merge_expr) => {
                let merge_type = self.infer_expr(merge_expr)?;
                match (input_type, &merge_type) {
                    (Type::Tuple(left_elems), Type::Tuple(right_elems)) => {
                        let mut elems = left_elems.clone();
                        elems.extend(right_elems.clone());
                        Ok(Type::Tuple(elems))
                    }
                    (left, Type::Tuple(right_elems)) => {
                        let mut elems = vec![left.clone()];
                        elems.extend(right_elems.clone());
                        Ok(Type::Tuple(elems))
                    }
                    (Type::Tuple(left_elems), right) => {
                        let mut elems = left_elems.clone();
                        elems.push(right.clone());
                        Ok(Type::Tuple(elems))
                    }
                    (left, right) => Ok(Type::Tuple(vec![left.clone(), right.clone()])),
                }
            }

            PipelineStep::MatchArm(_) => {
                Err("Match arm type checking not yet implemented".to_string())
            }
            PipelineStep::ForLoop(_) => {
                Err("For loop type checking not yet implemented".to_string())
            }
            PipelineStep::ArithmeticBinaryOp { op: _, right } => {
                let right_type = self.infer_expr(right)?;
                let normalized_input = input_type.normalize();
                let normalized_right = right_type.normalize();

                if !types_compatible(&normalized_input, &Type::Int) {
                    return Err(format!(
                        "Arithmetic left operand must be Int, got {}",
                        input_type
                    ));
                }
                if !types_compatible(&normalized_right, &Type::Int) {
                    return Err(format!(
                        "Arithmetic right operand must be Int, got {}",
                        right_type
                    ));
                }

                Ok(Type::Int)
            }
        }
    }
}
