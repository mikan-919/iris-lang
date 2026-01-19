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

Irisプロジェクトでは、**Trunk-Based Development (TBD)** と **GitHub Flow** を組み合わせた戦略を採用します。

### 1. Branch Roles
- `main`: 安定版ブランチ。マイルストーン達成時のみ `dev` からマージされる。直接のコミットは禁止。
- `dev`: 開発の本流（Trunk）。全ての機能開発はこのブランチを起点とし、ここへのマージを目指す。常にテストがパスしている必要がある。
- `feature/#<issue>-<desc>`/`vk/<random>-<desc>`: 短寿命な作業ブランチ。寿命は1〜2日とし、完了後は速やかに `dev` へマージする。

### 2. Implementation Steps (Mandatory)
タスクを実行する際、エージェントは必ず以下のステップを踏むこと：

1.  Issueの作成: `gh issue create` を使用し、タスクを最小単位にIssue化する。
2.  ブランチの作成: `dev` ブランチから `feature/#<Issue番号>-<概要>` ブランチを作成し、切り替える。
    - もし現在居るブランチが`vk/~`であれば、新しくブランチを作成する必要はありません。
3.  インクリメンタルな開発:
    - 小さな単位で実装とテストを繰り返す。
    - コミットメッセージは Conventional Commits (`feat:`, `fix:`, `test:`, `refactor:`) に従う。
4.  Pull Request (PR) の作成:
    - 実装完了後、`gh pr create` を使用して `dev` ブランチへのPRを作成する。
    - PRの説明には「何を解決したか」「どのテストをパスしたか」を記述する。
5.  マージ後のクリーンアップ: PRがマージされたことをユーザーから受け取ったら、ローカルおよびリモートの作業ブランチを削除する。

### 3. Coding Standards
- Small Changes: 1つのPRで巨大な機能を実装しない。レビュー可能なサイズに分割する。
  - これはユーザーから指示されたタスクが大きすぎる場合に勝手に小さいPRにすることを許可することを意味する。
- Test-Driven: 新機能には必ず対応するテストを含め、既存のテストを壊していないことを確認する。
- Doc-Sync: 言語仕様（`IRIS_SPEC.md`）に変更が及ぶ場合、必ずドキュメントも同時に更新する。

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
