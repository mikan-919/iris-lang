use crate::ast::Type;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum OwnershipState {
    Available,
    Consumed,
    Borrowed(String),
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
    pub ownership: OwnershipState,
    pub is_function: bool,
}

#[derive(Debug, Clone)]
pub struct Scope {
    symbols: HashMap<String, Symbol>,
    parent: Option<Box<Scope>>,
}

impl Scope {
    pub fn new(parent: Option<Box<Scope>>) -> Self {
        Self {
            symbols: HashMap::new(),
            parent,
        }
    }

    pub fn insert(&mut self, name: String, symbol: Symbol) {
        self.symbols.insert(name, symbol);
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.get(name)))
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        if self.symbols.contains_key(name) {
            self.symbols.get_mut(name)
        } else {
            self.parent.as_mut().and_then(|p| p.get_mut(name))
        }
    }

    pub fn child_scope(self) -> Scope {
        Scope::new(Some(Box::new(self)))
    }
}

#[derive(Debug)]
pub struct SymbolTable {
    root: Scope,
    current: Scope,
}

impl SymbolTable {
    pub fn new() -> Self {
        let root = Scope::new(None);
        let current = Scope::new(Some(Box::new(root.clone())));
        Self { root, current }
    }

    pub fn push_scope(&mut self) {
        let child = std::mem::replace(&mut self.current, Scope::new(None));
        self.current = Scope::new(Some(Box::new(child)));
    }

    pub fn pop_scope(&mut self) {
        if self.current.parent.is_some() {
            let parent = self.current.parent.take().unwrap();
            self.current = *parent;
        }
    }

    pub fn define(&mut self, name: String, ty: Type, is_function: bool) {
        let symbol = Symbol {
            name: name.clone(),
            ty,
            ownership: OwnershipState::Available,
            is_function,
        };
        self.current.insert(name, symbol);
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.current.get(name)
    }

    pub fn consume(&mut self, name: &str) -> Result<(), String> {
        if let Some(symbol) = self.current.get_mut(name) {
            match &symbol.ownership {
                OwnershipState::Available => {
                    symbol.ownership = OwnershipState::Consumed;
                    Ok(())
                }
                OwnershipState::Consumed => Err(format!("Variable '{}' already consumed", name)),
                OwnershipState::Borrowed(_) => {
                    Err(format!("Variable '{}' is borrowed, cannot consume", name))
                }
            }
        } else {
            Err(format!("Undefined variable '{}'", name))
        }
    }

    pub fn borrow(&mut self, name: &str, ref_name: String) -> Result<(), String> {
        if let Some(symbol) = self.current.get_mut(name) {
            match &symbol.ownership {
                OwnershipState::Available => {
                    symbol.ownership = OwnershipState::Borrowed(ref_name);
                    Ok(())
                }
                OwnershipState::Consumed => Err(format!("Variable '{}' already consumed", name)),
                OwnershipState::Borrowed(_) => Err(format!("Variable '{}' already borrowed", name)),
            }
        } else {
            Err(format!("Undefined variable '{}'", name))
        }
    }
}
