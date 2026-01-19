# PARSER MODULE

## OVERVIEW
Transforms lexer tokens into AST statements with recursive descent parsing.

## STRUCTURE
- `mod.rs` - Main parser implementation with recursive descent functions
- `tests.rs` - Parser unit tests covering all statement/expression types

## WHERE TO LOOK
- Entry: `Parser::parse()` → `Result<Vec<Stmt>>`
- Statement parsing: `parse_statement()`, `parse_binding()`, `parse_function_definition()`
- Expression parsing: `parse_expression()`, `parse_binary_op()`, `parse_term()`
- Token utilities: `consume()`, `check()`, `match_token()`, `advance()`

## CONVENTIONS
- Recursive descent: Each parse function handles one grammar rule
- Error messages include token location + expected vs actual
- Skip whitespace/newline during parsing, never emit AST nodes for them
- Expressions parsed bottom-up: terms → binary ops → pipeline steps
- Pattern matching uses `match` on `TokenKind` clones for ownership

## ANTI-PATTERNS
- Don't manually check `self.current` bounds; use `is_at_end()` helper
- Don't panic on parse errors; return `Result<T, String>` always
- Don't create empty stmts for whitespace - skip silently
- Don't clone tokens unnecessarily; prefer `check()`/`match_token()` helpers
