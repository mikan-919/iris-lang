---
name: parser-slice-assumptions
description: docsに明記がなくフロントエンド縦切り実装で独自に決めた構文の仮定
metadata:
  type: project
---

iris-lang のフロントエンド縦切り（字句解析→構文解析、nom+miette）実装で、docs/spec に明記がないため独自に決めた点。ユーザー確認が望ましい。

- **再代入の `mut` 位置**: `let mut x = ...`（Rust風）と仮定。docs は「mut は型修飾子」とも書くため `let x: mut T` の可能性もある。
- **代入文**: `target = value` を文として追加（docs に文法記述なし）。
- **改行**: 連続改行は文区切りとしてまとめて読み飛ばす。`else` は `}` と同じ行に必要（改行をまたぐ else は未対応）。
- **行コメント `//`** のみ対応（ブロックコメント未対応）。文字列補間（バッククォート）も未実装。

**Why:** 言語仕様が未確定の領域で実装を進めるための暫定判断。
**How to apply:** これらの構文を本実装する前にユーザーに確認する。確定したら docs/spec を更新する。
