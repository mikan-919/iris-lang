use crate::ast::Expr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Variable(pub String);

#[derive(Debug, Clone, PartialEq)]
pub enum IrInstruction {
    LoadConst {
        reg: String,
        value: Expr,
    },
    Assign {
        var: Variable,
        reg: String,
    },
    Return {
        reg: String,
    },
    Call {
        func_name: String,
        args: Vec<String>,
        target: String,
    },
    Branch {
        cond: String,
        true_block: String,
        false_block: String,
    },
    Panic {
        msg: String,
    },
    Exit {
        value: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct BasicBlock {
    pub label: Option<String>,
    pub instructions: Vec<IrInstruction>,
}

impl BasicBlock {
    pub fn new() -> Self {
        Self {
            label: None,
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

    #[test]
    fn test_call_instruction() {
        let instr = IrInstruction::Call {
            func_name: "double".to_string(),
            args: vec!["t0".to_string()],
            target: "t1".to_string(),
        };

        if let IrInstruction::Call {
            func_name,
            args,
            target,
        } = &instr
        {
            assert_eq!(func_name, "double");
            assert_eq!(args, &vec!["t0".to_string()]);
            assert_eq!(target, "t1");
        } else {
            panic!("Expected Call instruction");
        }
    }

    #[test]
    fn test_branch_instruction() {
        let instr = IrInstruction::Branch {
            cond: "t0".to_string(),
            true_block: "bb_true".to_string(),
            false_block: "bb_false".to_string(),
        };

        if let IrInstruction::Branch {
            cond,
            true_block,
            false_block,
        } = &instr
        {
            assert_eq!(cond, "t0");
            assert_eq!(true_block, "bb_true");
            assert_eq!(false_block, "bb_false");
        } else {
            panic!("Expected Branch instruction");
        }
    }

    #[test]
    fn test_panic_instruction() {
        let instr = IrInstruction::Panic {
            msg: "Error: Value is None".to_string(),
        };

        if let IrInstruction::Panic { msg } = &instr {
            assert_eq!(msg, "Error: Value is None");
        } else {
            panic!("Expected Panic instruction");
        }
    }

    #[test]
    fn test_exit_instruction() {
        let instr = IrInstruction::Exit {
            value: "t0".to_string(),
        };

        if let IrInstruction::Exit { value } = &instr {
            assert_eq!(value, "t0");
        } else {
            panic!("Expected Exit instruction");
        }
    }

    #[test]
    fn test_basic_block_with_label() {
        let mut block = BasicBlock::new();
        block.label = Some("entry".to_string());

        block.add(IrInstruction::LoadConst {
            reg: "t0".to_string(),
            value: Expr::Literal(Literal::Integer(10)),
        });

        assert_eq!(block.label, Some("entry".to_string()));
        assert_eq!(block.instructions.len(), 1);
    }
}
