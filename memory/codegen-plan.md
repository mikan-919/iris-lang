---
name: codegen-plan
description: iris-lang の LLVM コード生成の方式と現在地
metadata:
  type: project
---

iris-lang のバックエンドは LLVM（仕様）。この環境には `llvm-config` が無く inkwell/llvm-sys が使えない（`clang` 22 はあり `/usr/sbin/clang`）。そのため **自前でテキスト LLVM IR（`.ll`）を出力**し、`clang file.ll -o out` で実行ファイル化する方式を採用。JIT（ORC/MCJIT）は将来 LLVM 開発環境を入れたとき。

- 実装: `src/codegen.rs`（`compile_ir`、`tests/codegen.rs`）。型検査・所有権検査を通った AST → IR テキスト。
- 最初のスライスは確実性のため **i32 / bool 限定**。関数・引数・再帰、`let`/再代入/`return`、算術 `+ - * / %`、比較、論理 `&& ||`（短絡）、単項 `-`、`if`、三項。
- ローカルは alloca + load/store（SSA化は LLVM の mem2reg 任せ）。opaque ポインタ（`ptr`）で出力。
- 検証: `main(): i32` の戻り値＝終了コード。clang が無い環境では実行テストはスキップ。CLI `iris --emit-llvm <file>`。
- 二段ビルド driver（main.rs）: `iris build [--release] [-o OUT] <file>` と `iris run <file>`。既定 -O0（開発・高速）、`--release` で -O2。clang を呼んで実行ファイル化（`-Wno-override-module`）。計測: 800関数で -O0 156ms vs -O2 1468ms（約9倍）。コンパイル速度の最大レバーは「既定で最適化を軽くする」こと（LLVM最適化が支配項）。
- I/O と std: `extern fn`（本体なし＝C関数宣言、AST は Function.is_extern、codegen は `declare` 出力）を追加。`std/std.iris` を iris 自身で記述（putchar を借りて print_int/println_int/println_bool 等）。`analyze()` が `STD`（include_str!）をソース先頭へ連結（単一文字列で span 一意）。clang が libc をリンク→ `main(): i32` から実際に出力して実行できる（`tests/std_io.rs` で stdout 検証）。
- 未対応（次段）: f64 等の型・幅、struct/enum・メンバ/構造体リテラル、参照、`!`、文字列、ジェネリクス。WASM、JIT。

**Why:** llvm-config 不在で inkwell が使えず、テキスト IR なら依存ゼロで確実・テスト可能。I/O は extern+libc で最小実現。
**How to apply:** コード生成を広げるときは llvm_ty() の対応型を増やし、struct/参照のメモリ表現を設計。std は i32/bool 範囲で拡張。意味解析側は [[sema-plan]]。
