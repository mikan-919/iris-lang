# Iris Language

Wasm-first, performance-oriented language designed for maximum readability through **Vertical Data-Flow** and **Symbolic Backbone** syntax.

## Quick Start

### Example Code

```iris
export fn add(a: Int, b: Int) -> Int =: a + b
```

### Compilation

```bash
# Build the compiler
cargo build

# Run tests
cargo test
```

## Implementation Status

See [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) for detailed implementation status and test coverage.

### Current Features (36% Complete)

**Working:**
- Basic arithmetic operations (`+`, `-`, `*`, `/`)
- Function definition and export
- Int type and literals
- WASM compilation and execution
- Type checking

**Not Implemented:**
- Pipeline operator (`::`)
- Control flow (`match`, `if`)
- Error handling (`:`, `:?`)
- Async operations (`:~`)
- Borrowing and mutation
- Other types (Float, Bool, String)

## Development

### Build

```bash
cargo build
```

### Run Tests

```bash
cargo test
```

### Examples

Run debug examples to analyze compilation:

```bash
cargo run --example debug_wasm
cargo run --example test_main
cargo run --example debug_pipeline
```

## Specification

See [IRIS_SPEC.md](IRIS_SPEC.md) for the complete language specification.

## Project Structure

- `src/` - Source code
  - `lexer/` - Tokenizer
  - `parser/` - Parser
  - `ast/` - Abstract Syntax Tree
  - `analyzer/` - Type checker
  - `ir/` - Intermediate Representation
  - `codegen/` - WASM code generation
- `tests/` - Integration and execution tests
- `examples/` - Debug and test examples
- `docs/` - Documentation

## License

MIT
