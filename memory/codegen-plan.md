---
name: codegen-plan
description: iris-lang の LLVM コード生成の方式と現在地
metadata:
  type: project
---

iris-lang のバックエンドは LLVM（仕様）。この環境には `llvm-config` が無く inkwell/llvm-sys が使えない（`clang` 22 はあり `/usr/sbin/clang`）。そのため **自前でテキスト LLVM IR（`.ll`）を出力**し、`clang file.ll -o out` で実行ファイル化する方式を採用。JIT（ORC/MCJIT）は将来 LLVM 開発環境を入れたとき。

- 実装: `src/codegen.rs`（`compile_ir`、`tests/codegen.rs`）。型検査・所有権検査を通った AST → IR テキスト。
- 対応範囲は **数値プリミティブ＋bool**。関数・引数・再帰、`let`/再代入/`return`、算術 `+ - * / %`、比較、論理 `&& ||`（短絡）、単項 `-`、`if`、三項。
- 数値型を拡張済み: 整数 `i8..u64`・浮動小数 `f32`/`f64`。LLVM 整数型は符号を持たない（幅だけ）ため、`NumKind`(SInt/UInt/Float) を iris の `Ty` から導いて命令を選ぶ（`sdiv`/`udiv`/`fdiv`、`icmp slt`/`ult`/`fcmp o*`、`fneg`）。浮動小数リテラルは「正確に表現できる double のビット列」`0x{:016X}` で出力（float も同様）。零値ヘルパで既定 return を型ごとに切替。`tests/codegen.rs` に IR検査＋clang実行（float/udiv）の回帰を追加。
- ローカルは alloca + load/store（SSA化は LLVM の mem2reg 任せ）。opaque ポインタ（`ptr`）で出力。
- 検証: `main(): i32` の戻り値＝終了コード。clang が無い環境では実行テストはスキップ。CLI `iris --emit-llvm <file>`。
- 二段ビルド driver（main.rs）: `iris build [--release] [-o OUT] <file>` と `iris run <file>`。既定 -O0（開発・高速）、`--release` で -O2。clang を呼んで実行ファイル化（`-Wno-override-module`）。計測: 800関数で -O0 156ms vs -O2 1468ms（約9倍）。コンパイル速度の最大レバーは「既定で最適化を軽くする」こと（LLVM最適化が支配項）。
- I/O と std: `extern fn`（本体なし＝C関数宣言、AST は Function.is_extern、codegen は `declare` 出力）を追加。`std/std.iris` を iris 自身で記述（putchar を借りて print_int/println_int/println_bool 等）。`analyze()` が `STD`（include_str!）をソース先頭へ連結（単一文字列で span 一意）。clang が libc をリンク→ `main(): i32` から実際に出力して実行できる（`tests/std_io.rs` で stdout 検証）。
- **参照 `&T` / `&mut T`** を追加: LLVM では opaque ポインタ `ptr`（`llvm_ty(Ty::Ref)=ptr`、`zero_value(ptr)=null`）。`&x`/`&mut x` は `place_ptr(inner)` でローカル/引数の alloca アドレスを値として返す。値が期待される文脈では**暗黙デリファレンス**（ユーザー合意の設計）: `gen_value(expr, want)` ヘルパが、`want` が値型のとき参照を 1 段ずつ `load` で剥がす（多段参照対応）。`want` が参照型/`Infer` のときは参照のまま保つ（`let r = &x` は束縛）。リテラル幅は `want` をヒントに確定。経路を通した箇所: `let`/`return`(`ret_ty` フィールド追加)/`assign`/呼び出し引数/二項被演算子(`operand_ty` も peel して chosen Ty を返す)/単項`-`/条件(if/while/三項)。typeck 側も `&T`→`T` を受理: `assignable`(actual が Ref で expected が非 Ref なら inner で再判定)・`join_numeric`/`expect_bool` で `Ty::peel_refs()`。`tests/codegen.rs` に IR検査＋実行(42/8/30)の回帰追加。
- 参照越しの代入（write-through `&mut`）は今回**未対応**: 参照変数への再代入は束縛の付け替え。読み出し（auto-deref）だけで観測可能なテストは成立するため次段送り。
- **struct** を追加（非ジェネリックのみ）: `StructReg`（codegen.rs）が型定義から `name→fields` と別名 `A→B` を集め、`%Name = type { ... }` をモジュール先頭に宣言。`llvm_ty(ty, reg)` に reg を通して struct/別名（struct/プリミティブ双方）を解決。struct 値は **first-class 値**（引数・戻り値・let で値渡し。`load %Name`/`ret %Name`/`call ... %Name`）。構造体リテラル=alloca→各フィールド `getelementptr inbounds %Name, ptr, i32 0, i32 IDX`+store→全体 load。メンバアクセス=`field_ptr`(GEP)+load。`place_ptr` を Member 対応にしたので `a.b = v`（ローカル）・`&a.b` も成立。`struct_base_ptr` が「参照越し（ポインタを辿る）/場所(place_ptr)/場所でない値(一時 alloca に退避)」を吸収し、ネスト `l.a.x` と `origin().y`（関数戻り値のメンバ）に対応。`zero_value` は `%` 始まりで `zeroinitializer`。`tests/codegen.rs` に IR検査＋clang実行(42/19)の回帰6本。
- **文字列 `string`** を追加: C 風の **NUL 終端**表現（ユーザー合意: 表現は C-style null-terminated `ptr`、スコープは「リテラル＋printable end-to-end」）。`llvm_ty("string")=ptr`。文字列リテラルは内容ごとに `StringPool`（codegen.rs、`RefCell` で全関数共有）が `@.str.N = private unnamed_addr constant [N x i8] c"...\00"` を割り当て、`gen_expr(Str)` はその記号（opaque ポインタなのでそのまま `ptr` 値）を返す。グローバル定義はモジュール末尾に出力（IR では順序自由）。`encode_cstr`: 印字可能ASCII以外と `"` `\` を `\XX` でエスケープ＋末尾 NUL（UTF-8 はバイト単位）。値渡し（引数/戻り値/let）は既存の `gen_value` 経路でそのまま通る。std に `extern fn puts(s: string): i32` を追加し libc で出力（`tests/std_io.rs` で stdout 検証、`tests/codegen.rs` で IR/エスケープ検証）。**長さ・索引・連結・補間などの文字列操作は未実装**（言語にまだ演算が無い）。
- 未対応（次段）: enum（バリアント構築/分解は frontend も未対応）、参照越し代入(write-through)、`!`、文字列の操作（長さ・索引・連結・補間）、ジェネリクス。WASM、JIT。

**Why:** llvm-config 不在で inkwell が使えず、テキスト IR なら依存ゼロで確実・テスト可能。I/O は extern+libc で最小実現。
**How to apply:** コード生成を広げるときは llvm_ty() の対応型を増やし、struct/参照のメモリ表現を設計。std は i32/bool 範囲で拡張。意味解析側は [[sema-plan]]。
