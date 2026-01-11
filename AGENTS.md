# PROJECT KNOWLEDGE BASE

**Generated:** 2026-01-11
**Commit:** 83bed07
**Branch:** issue-1

## OVERVIEW
Rust programming language compiler (iris-lang) - greenfield project, initialization phase.

## STRUCTURE
```
./
└── .git/    # Git infrastructure only - no source code yet
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Initialize project | Run `cargo init --lib` or `cargo init --bin` | |
| Standard Rust structure | src/main.rs or src/lib.rs | Create with cargo init |

## CODE MAP
*(No code yet - will populate after initialization)*

## CONVENTIONS
Project not yet initialized. Standard Rust conventions will apply:
- Binary → `src/main.rs`
- Library → `src/lib.rs`
- Integration tests → `tests/` directory
- Unit tests → `mod` or `#[cfg(test)]` inline

## ANTI-PATTERNS (THIS PROJECT)
*(Not applicable - no code yet)*

## UNIQUE STYLES
*(Not applicable - no patterns established)*

## COMMANDS
```bash
cargo new <name>        # Create new Rust project
cargo init              # Initialize in current directory
cargo build             # Build
cargo test              # Run tests
cargo fmt               # Format code
cargo clippy            # Lint
```

## NOTES
- Repository is freshly initialized with git but no Rust project exists
- Issue #1: "最初の開発" tracks initial development
- Current branch: `issue-1` (from dev branch)
- Consider: `cargo init --lib` (compiler) vs `--bin` (CLI tool)
