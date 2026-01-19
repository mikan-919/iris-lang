# PROJECT KNOWLEDGE BASE

**Generated:** 2026-01-19
**Commit:** n/a
**Branch:** n/a

## OVERVIEW
WASM-first Rust compiler for Iris language (36% complete). Library-only crate (cdylib+rlib) with web frontend.

## STRUCTURE
```
./
├── src/
│   ├── lexer/      # Tokenization (tokenizer.rs + tests)
│   ├── parser/     # AST parsing (mod.rs + tests)
│   ├── ast/        # AST definitions (mod.rs)
│   ├── analyzer/   # Type checking (type_checker, symbol, errors)
│   ├── ir/         # IR generation (generator.rs)
│   ├── codegen/    # WASM bytecode generation (mod.rs - 17KB)
│   └── lib.rs      # Entry point: compile_to_wasm()
├── tests/          # Integration + execution tests
├── examples/       # Debug utilities (wasmtime execution)
├── iris-web/       # Vite + TypeScript frontend
└── pkg/            # Generated WASM bindings
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Compilation entry | src/lib.rs | compile_to_wasm() exported via wasm-bindgen |
| Language tokens | src/lexer/mod.rs | TokenKind enum defines all syntax |
| Type errors | src/analyzer/errors.rs | TypeError types |
| WASM generation | src/codegen/mod.rs | WasmGenerator::compile() |
| Integration tests | tests/*.rs | Full compilation cycle |
| Debug tools | examples/debug_*.rs | WASM inspection |

## Development Workflow & Branching Strategy

**Strategy**: Trunk-Based Development + GitHub Flow

### Branches
| Branch | Purpose | Rules |
|--------|---------|-------|
| `main` | Stable releases | Merged from `dev` only. No direct commits |
| `dev` | Development trunk | Always test-passing. All feature branches target this |
| `feat/<description>` | Task work | Created from `dev` for any code changes. Short-lived |

---

### Task Execution Flow (MANDATORY)

For any task that involves code changes:

```
1. CHECKOUT → 2. DEVELOP → 3. PUSH → 4. CREATE PR → 5. CLEANUP (after merge)
```

#### Step 1: Create Feature Branch
```bash
git checkout dev
git pull origin dev
git checkout -b feat/<description>
```

#### Step 2: Develop Incrementally
- Work in small, testable units
- Commit frequently with Conventional Commits: `feat:`, `fix:`, `test:`, `refactor:`
- Run tests after meaningful changes

#### Step 3: Push Branch
```bash
git push -u origin feat/<description>
```

#### Step 4: Create PR to `dev`
```bash
gh pr create --base dev
```
PR description must include:
- What was solved
- Which tests pass

#### Step 5: Cleanup (after user confirms merge)
```bash
git checkout dev
git pull origin dev
git branch -D feat/<description>
git push origin --delete feat/<description>
```

---

### Coding Standards

- **Small PRs**: Split large tasks into reviewable chunks. If user asks for too much at once, break it down yourself.
- **Test-Driven**: New features require tests. Verify existing tests still pass.
- **Doc-Sync**: If `IRIS_SPEC.md` is affected, update documentation together with code.

## CONVENTIONS
- Library-only crate: no main.rs
- wasm-bindgen for JS interop
- Module pattern: mod.rs re-exports public API
- Test files: *_tests.rs within modules, or tests/ directory
- Examples: cargo run --example <name>

## ANTI-PATTERNS (THIS PROJECT)
- No CLI binary entry point (library-only design)
- Mixed test locations: module tests vs tests/ directory (intentional separation)

## COMMANDS
```bash
cargo build              # Build (cdylib for WASM, rlib for tests)
cargo test               # Run all tests
cargo run --example test_main      # Native WASM execution
cargo run --example debug_wasm     # Inspect WASM bytecode
cd iris-web && npm run dev          # Web playground
```

## NOTES
- WASM target: wasm32-unknown-unknown
- Dev-dependency: wasmtime (for native testing)
- Web frontend loads from pkg/iris_lang.js
- Build script (build.rs) runs wasm-bindgen automatically
