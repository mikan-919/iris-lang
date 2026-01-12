# PROJECT KNOWLEDGE BASE

## OVERVIEW
Rust programming language compiler (iris-lang) - greenfield project, initialization phase.

## COMMANDS
```bash
cargo new <name>        # Create new Rust project
cargo init              # Initialize in current directory
cargo build             # Build
cargo test              # Run tests
cargo fmt               # Format code
cargo clippy            # Lint
```

## Development Workflow & Branching Strategy

Irisプロジェクトでは、**Trunk-Based Development (TBD)** と **GitHub Flow** を組み合わせた戦略を採用します。

### 1. Branch Roles
- **`main`**: 安定版ブランチ。マイルストーン達成時のみ `dev` からマージされる。直接のコミットは禁止。
- **`dev`**: 開発の本流（Trunk）。全ての機能開発はこのブランチを起点とし、ここへのマージを目指す。常にテストがパスしている必要がある。
- **`feature/#<issue>-<desc>`**: 短寿命な作業ブランチ。寿命は1〜2日とし、完了後は速やかに `dev` へマージする。

### 2. Implementation Steps (Mandatory)
タスクを実行する際、エージェントは必ず以下のステップを踏むこと：

1.  **Issueの作成**: `gh issue create` を使用し、タスクを最小単位でIssue化する。
2.  **ブランチの作成**: `dev` ブランチから `feature/#<Issue番号>-<概要>` ブランチを作成し、切り替える。
3.  **インクリメンタルな開発**:
    - 小さな単位で実装とテストを繰り返す。
    - コミットメッセージは Conventional Commits (`feat:`, `fix:`, `test:`, `refactor:`) に従う。
4.  **Pull Request (PR) の作成**: 
    - 実装完了後、`gh pr create` を使用して `dev` ブランチへのPRを作成する。
    - PRの説明には「何を解決したか」「どのテストをパスしたか」を記述する。
5.  **マージ後のクリーンアップ**: PRがマージされたことをユーザーから受け取ったら、ローカルおよびリモートの作業ブランチを削除する。

### 3. Coding Standards
- **Small Changes**: 1つのPRで巨大な機能を実装しない。レビュー可能なサイズに分割する。
  - これはユーザーから指示されたタスクが大きすぎる場合に勝手に小さいPRにすることを許可することを意味する。
- **Test-Driven**: 新機能には必ず対応するテストを含め、既存のテストを壊していないことを確認する。
- **Doc-Sync**: 言語仕様（`IRIS_SPEC.md`）に変更が及ぶ場合、必ずドキュメントも同時に更新する。