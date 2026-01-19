# src/analyzer/ - Type Checking & Semantic Analysis

## OVERVIEW
Type checking and semantic analysis with ownership tracking

## STRUCTURE
```
src/analyzer/
├── mod.rs             # Public API: exports TypeChecker, SymbolTable, TypeError, OwnershipState
├── type_checker.rs    # Main type checker with scope management
├── symbol.rs          # Symbol table with hierarchical scopes (Scope + SymbolTable)
├── errors.rs          # TypeError enum (8 variants)
└── tests.rs           # Unit tests
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Type checking entry | type_checker.rs | TypeChecker::check() |
| Symbol resolution | symbol.rs | SymbolTable::get() walks parent scopes |
| Ownership states | symbol.rs | OwnershipState enum (Available/Consumed/Borrowed) |
| Type errors | errors.rs | TypeError variants for all semantic errors |
| Function signatures | type_checker.rs | HashMap<String, Type> in TypeChecker |

## CONVENTIONS
- SymbolTable uses parent pointer pattern (Scope with Option<Box<Scope>>)
- Scope push/pop for function blocks
- Error messages via std::fmt::Display
- Type compatibility: Type::Any matches everything
- Ownership tracking per symbol (currently unenforced)

## ANTI-PATTERNS
- Direct Scope access (use SymbolTable API instead)
- Manual scope management (always push_scope/pop_scope pairs)
- Unchecked ownership state changes (OwnershipState tracked but not enforced)
