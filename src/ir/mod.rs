use crate::ast::Expr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Variable(pub String);

#[derive(Debug, Clone, PartialEq)]
pub enum IrInstruction {
    LoadConst { reg: String, value: Expr },
    Assign { var: Variable, reg: String },
    Return { reg: String },
}

#[derive(Debug, Clone, Default)]
pub struct BasicBlock {
    pub instructions: Vec<IrInstruction>,
}

impl BasicBlock {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    pub fn add(&mut self, instr: IrInstruction) {
        self.instructions.push(instr);
    }

    pub fn iter(&self) -> impl Iterator<Item = &IrInstruction> {
        self.instructions.iter()
    }
}

pub mod generator;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Literal;

    #[test]
    fn test_load_const() {
        let instr = IrInstruction::LoadConst {
            reg: "t0".to_string(),
            value: Expr::Literal(Literal::Integer(10)),
        };

        if let IrInstruction::LoadConst { reg, value } = &instr {
            assert_eq!(reg, "t0");
            assert!(matches!(value, Expr::Literal(Literal::Integer(10))));
        } else {
            panic!("Expected LoadConst instruction");
        }
    }

    #[test]
    fn test_assign() {
        let instr = IrInstruction::Assign {
            var: Variable("x".to_string()),
            reg: "t0".to_string(),
        };

        if let IrInstruction::Assign { var, reg } = &instr {
            assert_eq!(var.0, "x");
            assert_eq!(reg, "t0");
        } else {
            panic!("Expected Assign instruction");
        }
    }

    #[test]
    fn test_basic_block() {
        let mut block = BasicBlock::new();

        block.add(IrInstruction::LoadConst {
            reg: "t0".to_string(),
            value: Expr::Literal(Literal::Integer(10)),
        });

        block.add(IrInstruction::Assign {
            var: Variable("x".to_string()),
            reg: "t0".to_string(),
        });

        assert_eq!(block.instructions.len(), 2);
    }
}
