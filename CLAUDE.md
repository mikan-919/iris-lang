# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Run a file (evaluate and print result)
go run cli/iris/main.go playground/sample.iris

# Print the AST only (no evaluation)
go run cli/iris/main.go --ast playground/sample.iris

# Print the token stream only
go run cli/iris/main.go --lex playground/sample.iris

# Interactive REPL (blank line to submit, :lex to toggle lex mode)
go run cli/iris/main.go --repl

# Build all packages
go build ./...

# Run all tests
go test ./...

# Run tests for a specific package
go test ./lexer/...
go test ./parser/...
go test ./eval/...
go test ./typechecker/...
```

Go version is managed via `mise` (`mise.toml`).

## Architecture

Iris is a subject-centric, flow-based language. Its central concept is the **backbone operators** that thread a "subject" through a chain of steps:

```iris
text          // subject
:: trim()     // ::  continue (sync)
:> snapshot   // :>  borrow-save
:: split(",") // ::  continue
```

**Language spec: `IRIS_SPEC_2.md` is the sole canonical reference.** `IRIS_SPEC.md` (v1.1) is deprecated and should be ignored.

### Backbone operators

Two categories, both defined in `IRIS_SPEC_2.md`:

**Symbol-suffix ops** (2-char):

| Op | Name | Meaning |
|---|---|---|
| `=:` | Initiate | bind / start pipeline |
| `::` | Next | continue sync |
| `:~` | Await | continue async (compat alias for `:await`) |
| `:^` | Try | propagate error (short-circuit on error) |
| `:!` | Force | panic on error / unwrap |
| `:?` | Catch | handle error (positional) |
| `:\|` | Or | fallback value |
| `:>` | Tag | borrow-save (lifetime-bounded snapshot) |
| `:&` | Join | tuple join `(subject, tag)` |

**Keyword-suffix ops** (`:keyword`, from IRIS_SPEC_2.md):

| Op | Meaning |
|---|---|
| `:if cond :then body [:else fallback]` | Conditional branch |
| `:while cond :then body` | Loop while condition is truthy |
| `:then%` | Side-effect variant — body runs but subject is not replaced |
| `:catch [$alias] body` | Named error catch; `$alias` binds the error value |
| `:await` | Standalone async wait (currently treated as sync) |

### Token system

`token/token.go` is the single source of truth for all token kinds (`token.Kind`). Both the lexer and parser import this package.

`lexer/lexer.go` exports:
- `New(src string) *Lexer`
- `Next() token.Token` — returns one token, skipping whitespace and comments
- `AllTokens() []token.Token` — convenience for getting all tokens

The lexer recognises `:keyword` backbone ops by reading an alphabetic sequence after `:` and mapping it via `colonKeywordKind`.

### Package layout

| Package | Role |
|---|---|
| `token/` | Token kinds (`Kind` enum), `Token` struct, keyword map |
| `lexer/` | Lexer; `New(src) *Lexer`, `Next() token.Token` |
| `ast/` | All AST node types: statements, expressions, type expressions |
| `parser/` | Pratt parser; `New(l) *Parser`, `ParseProgram() *ast.Program` |
| `typechecker/` | Type inference, trait-bound checking, lifetime/tag analysis |
| `eval/` | Tree-walking interpreter; `New() *Evaluator`, `Exec(prog) Value` |
| `visitor/` | Stub — reserved for future AST visitor |
| `cli/iris/` | CLI: run / --ast / --lex / --repl modes |
| `main.go` | Dev entry point |

### Key AST nodes

- **`Pipeline`** — `Subject Expression` + `[]PipelineStep{Op, Expr}`; Subject is nil for headless pipelines
- **`LetStatement`** — `Name string` + `Value *Pipeline` (always starts with `=:`)
- **`FnDeclaration`** — supports type params (`<T: #Trait && #Trait2>`), expression bodies (`=: pipeline`)
- **`TraitDeclaration`** / **`ImplDeclaration`** — define/implement traits
- **`ComptimeDirective`** — `!name(args)` (e.g. `!require(#Readable)`)
- **`OperatorApplication`** — `:: * 2` style arithmetic within a pipeline step
- **`SubjectRef`** — `$` (subject placeholder) or `$name` (subject alias)
- **`IfExpression`** — groups `:if cond :then body [:else fallback]`
- **`WhileExpression`** — groups `:while cond :then body`
- **`CatchNamedExpression`** — `:catch [$alias] body`

### Runtime values (eval package)

| Type | Description |
|---|---|
| `IntValue` / `FloatValue` / `StringValue` / `BoolValue` | Scalars |
| `ArrayValue` | `[T]` |
| `TupleValue` | `:&` join result `(a, b)` |
| `NilValue` | unit |
| `ErrorValue` | runtime error, caught by `:?` / `:catch` |
| `FnValue` | user-defined closure |
| `FlowValue` | headless pipeline (callable) |

### Lifetime model

`:>` tag creates a snapshot of the current subject bound to the pipeline's region. Tags are tracked by `LifetimeTracker` in the `typechecker` package:
- `enterPipeline()` / `exitPipeline()` delimit a region; exiting deletes tags created within it
- `:&` join and `$name` subject-ref validate that the referenced tag is still alive
