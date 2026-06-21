# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

iris-lang は「Go のシンプルさで Rust の所有権ベースのメモリ安全性を得る」コンパイル言語の
コンパイラ。Rust 製（edition 2024）。設計コンセプトの核心は **所有権を呼び出し側に露出しない**
こと（ライフタイム注釈なし、所有権はコンパイラ内部の DAG で管理）。ドキュメントとコード内
コメントは日本語。新規ドキュメント・コメントも日本語で書く。

## ビルド・テスト・実行

```sh
cargo build
cargo test                          # 全テスト
cargo test --test typeck            # 単一テストファイル（tests/typeck.rs）
cargo test --test typeck name       # ファイル内の特定テスト名で絞り込み

cargo run -- examples/hello.iris            # AST を表示（デフォルト動作）
cargo run -- --emit-llvm examples/print.iris  # LLVM IR（テキスト）を表示
cargo run -- run examples/print.iris        # 生成して即実行（デフォルト -O0）
cargo run -- build [--release] [-o OUT] examples/print.iris  # 実行ファイル生成
```

- **実行ファイル化には `clang` が必須**（IR テキストを `clang` に渡してリンク）。この環境には
  `llvm-config` が無いため inkwell/llvm-sys は使えず、codegen は **LLVM IR をテキストで出力する**
  方式をとっている。JIT は将来 LLVM 導入時。
- ビルドは既定 **-O0**（開発・高速）、`--release` で **-O2**。ビルド総時間は clang の最適化が
  支配するため、開発ループは -O0 が速い。
- ベンチ: `cargo run --release --example bench_compile` / `bench_ownership`。

## パイプライン構成

ソース → 字句解析 → トークン列 → 構文解析 → AST → 名前解決 → 型検査 → 所有権 DAG 検査 → LLVM IR → clang

エントリは `src/lib.rs` の `analyze()`。各段は前段の結果を受け取り、失敗時は
`src/diagnostics.rs` 経由で **miette** によるソース位置付き診断を返す。各意味解析段は
**エラーを 1 件で止めず複数件まとめて報告する**設計。

| 段 | ファイル | 役割 |
|---|---|---|
| 字句解析 | `src/lexer.rs` | nom 8 + nom_locate。span 付きトークン列を生成 |
| 構文解析 | `src/parser/` | `&[Token]` に nom `Input` を実装（`tokens.rs`）。式は優先順位ごとに段分け |
| 名前解決 | `src/sema/resolve.rs` | スコープ構築、使用→定義の対応。型名・フィールド名は扱わない（型検査側） |
| 型検査 | `src/sema/typeck.rs` + `ty.rs` | 内部型 `Ty` を付与。整数/小数リテラルは未確定型として適合 |
| 所有権 DAG | `src/sema/ownership/` | 三層検査（下記） |
| コード生成 | `src/codegen.rs` | LLVM IR テキスト出力 |

### 所有権 DAG（`src/sema/ownership/`）

仕様の核心。三層に分かれる:
1. **型レベルの所有グラフ＋循環検出**（`typegraph.rs`）— 無限サイズ型を検出。`&T`/`Box`/`Vec`
   等は所有辺を作らない。
2. **値レベルのムーブ＋借用グラフ・ライフタイム**（`flow.rs`）— 既定ムーブ、Copy 判定、
   ダングリング返却検出。ライフタイムは provenance を借用グラフで追跡し**自動推論**（ADR-0003、
   注釈構文は存在しない）。
3. **借用競合（エイリアス規則）**（`borrows.rs`）— `&mut` 排他 / `&` 複数可。生存期間は
   **スコープベース（保守的）**で、NLL のような精密さは未実装。

### 標準ライブラリ（`std/std.iris`）

**iris 自身で書かれた**最小 std。C の `putchar` / `puts` を `extern fn` で借りて I/O を実現。
`src/lib.rs` の `analyze()` が全プログラムの先頭へ**自動で前置**する（`STD` 定数を連結し、
span を一意に保つため単一文字列として扱う）。現状の codegen に合わせ `i32`/`bool`/`string` で記述。

## 言語設計上の確定事項（ADR）

`docs/adr/` 参照。Rust と異なる重要な選択:
- **`&` は参照専用、`+` が型合成**（ADR-0001）。`A + B` がトレイト境界/交差型。
- **`!` がエラー伝播**（早期 return、panic ではない）、**`?` は三項演算子** `cond ? a : b`（ADR-0002）。
- **ライフタイム注釈は書かない**（ADR-0003）。コンパイラが所有権 DAG から推論。
- 文区切りは**改行**（`;` なし）。`mut` は宣言側に置く型修飾子。名前的型付け。

## 実装範囲の注意

縦切りで実装中のため、構文・意味解析・codegen で対応範囲がずれる。**現状の正確な対応/未対応は
`docs/STATUS.md` を必ず参照**（最も詳細で最新）。要点:
- codegen は数値プリミティブ・`bool`・参照・struct・文字列（NUL 終端・リテラルの値渡しのみ）。
  **enum / `!` / 文字列の操作（長さ・索引・連結・補間）/ ジェネリクス / trait / match / for は未対応**。
- 参照は opaque ポインタ（`ptr`）で表現し、値が期待される文脈で**暗黙にデリファレンス**する。
  参照越しの代入（write-through）は未対応。
- `docs/STATUS.md` 末尾「仕様未確定のため独自に決めた点」に、docs に記述がなく実装側で暫定決定した
  構文（`let mut x`、代入文 `target = value`、`else` の位置）がある。本実装前にユーザー確認が必要。

## ドキュメントの所在

- `docs/CONCEPT.md` — 言語のコンセプト（不変の設計指針）。
- `docs/CONTEXT.md` — 用語集（trait/let/const/mut/所有権 DAG など）。
- `docs/spec/` — 言語仕様（type/trait/ownership/control/error/function/module/concurrency/compiler）。
- `docs/STATUS.md` — 実装進捗（このリポジトリで最も更新頻度が高い実態ドキュメント）。
- `docs/adr/` — 設計判断の記録。
