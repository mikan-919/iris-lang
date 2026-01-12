use crate::ast::{Expr, Stmt};
use crate::ir::{BasicBlock, IrInstruction, Variable};

pub fn generate_ir(stmt: &Stmt) -> BasicBlock {
    let mut block = BasicBlock::new();

    match stmt {
        Stmt::Binding { name, expr } => {
            let (reg, expr_block) = generate_expr_ir(expr);
            block.instructions.extend(expr_block);

            block.add(IrInstruction::Assign {
                var: Variable(name.clone()),
                reg,
            });
        }
        _ => {}
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
        _ => (String::new(), instructions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Literal;

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
}
