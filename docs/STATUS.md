# 実装状況

iris-lang コンパイラの実装進捗。最終更新: 2026-06-20。

## パイプライン

```
ソース ─字句解析→ トークン列 ─構文解析→ AST ─名前解決→ 型検査→ 所有権DAG検査→ LLVM IR生成→ clang
       (lexer.rs)          (parser/)       (resolve)  (typeck)  (ownership/)   (codegen.rs)  (実行ファイル)
```

現状はフロントエンドの **字句解析 → 構文解析 → AST → 名前解決 → 型検査 → 所有権 DAG 検査** までを縦切りで実装済み。
さらに **LLVM IR（テキスト `.ll`）生成**に着手済み（`i32` / `bool` の部分集合）。`clang` で実行ファイル化できる。
エラーはすべて [miette](https://github.com/zkat/miette) でソース位置付きで表示する。
字句・構文解析には [nom 8](https://github.com/rust-bakery/nom)（+ nom_locate）を用いる。

## ファイル構成

| ファイル | 役割 | 状態 |
|---|---|---|
| `src/span.rs` | ソース位置（offset/len）。miette `SourceSpan` へ変換 | ✅ |
| `src/token.rs` | トークン定義 | ✅ |
| `src/ast.rs` | 抽象構文木 | ✅（縦切り範囲） |
| `src/lexer.rs` | nom + nom_locate による字句解析（span付き） | ✅（縦切り範囲） |
| `src/parser/tokens.rs` | `&[Token]` への nom `Input` 実装 | ✅ |
| `src/parser/error.rs` | 構文解析エラー型 | ✅ |
| `src/parser/mod.rs` | 構文解析（式は優先順位ごとに段分け） | ✅（縦切り範囲） |
| `src/sema/resolve.rs` | 名前解決（スコープ構築・定義/使用の対応） | ✅（縦切り範囲） |
| `src/sema/ty.rs` | 型検査の内部型表現 `Ty` | ✅（縦切り範囲） |
| `src/sema/typeck.rs` | 型検査（型付け・整合性検査・可変性検査） | ✅（縦切り範囲） |
| `src/sema/ownership/` | 所有権 DAG（型グラフ循環検出＋ムーブ/借用グラフ＋借用競合） | ✅（縦切り範囲） |
| `src/codegen.rs` | LLVM IR（テキスト）生成 | ✅（i32 / bool） |
| `std/std.iris` | 最小の標準ライブラリ（iris 自身で記述・自動前置） | ✅ |
| `src/diagnostics.rs` | miette 診断 | ✅ |
| `src/lib.rs` / `src/main.rs` | ライブラリ / CLI（AST 表示・`--emit-llvm`・`build`/`run`） | ✅ |
| `tests/parse.rs` | 字句・構文解析の回帰テスト | ✅ |
| `tests/resolve.rs` | 名前解決の回帰テスト | ✅ |
| `tests/typeck.rs` | 型検査の回帰テスト | ✅ |
| `tests/typedef.rs` | 型定義・構造体リテラルの回帰テスト | ✅ |
| `tests/ownership.rs` | ムーブ検査の回帰テスト | ✅ |
| `tests/ownership_dag.rs` | 所有権 DAG 循環検出・ライフタイムの回帰テスト | ✅ |
| `tests/borrow_conflict.rs` | 借用競合（エイリアス規則）の回帰テスト | ✅ |
| `tests/codegen.rs` | LLVM IR 生成・clang 実行の回帰テスト | ✅ |
| `tests/std_io.rs` | std を使った出力プログラムの実行テスト | ✅ |

## 構文の実装状況

### 実装済み

- 関数定義 `fn name(params): RetType { ... }`（`pub`、戻り値型省略可、末尾カンマ可）
- 外部関数宣言 `extern fn name(params): RetType`（本体なし・C 関数を借りる。例: `putchar`）
- 型定義 `type Name<T> = (別名 | struct { ... } | enum { ... })`（`pub`、ジェネリクス、フィールド/バリアントは改行・カンマ区切り）
- 構造体リテラル `Name { field: value, ... }`（`if`/三項の条件位置では抑制し曖昧性回避）
- 文: `let` / `const`、`return`、再代入 `target = value`、式文
- ループ: `while cond { ... }` / `loop { ... }` / `break` / `continue`（`for` はイテレータ＝enum/trait 待ちで未対応）
- 型: 名前付き型、ジェネリクス `Vec<T>`、参照 `&T` / `&mut T`、配列 `T[]`、タプル `(A, B)`
- 式（優先順位対応）:
  - リテラル（整数・浮動小数点・文字列・真偽値）、識別子
  - 二項演算 `+ - * / %`、比較 `< <= > >=`、等価 `== !=`、論理 `&& ||`
  - 単項 `-`、参照 `&` / `&mut`
  - 後置: 関数呼び出し `f(...)`、メンバアクセス `a.b`、エラー伝播 `expr!`
  - 三項演算子 `cond ? a : b`
  - if 式（`else if` / `else` 連鎖）
- 改行による文区切り、空白・タブ・行コメント `//` の読み飛ばし
- 字句エラー・構文エラーの miette 表示（該当 span を指す）

### 意味解析（名前解決のみ実装済み）

`src/sema/resolve.rs`。スコープ（グローバル / 関数 / ブロック）を構築し、識別子の
使用位置を定義（関数・引数・ローカル変数）に結びつける。

- 関数名は先に一括登録するため、相互再帰・前方参照が可能
- 検出するエラー（複数件をまとめて miette で表示）:
  - 未定義の名前の使用
  - トップレベル関数名の重複定義
  - 同一関数内での引数名の重複
- ローカル変数の `let` 再宣言（シャドーイング）は許可
- `Ok` / `Err` / `Some` / `None` をプレリュード（組み込み名）として登録
- 型名・メンバ名・構造体リテラルのフィールド名の検証は型情報が必要なため型検査側で行う（名前解決は値の名前のみ）

### 型検査（実装済み）

`src/sema/ty.rs`（内部型 `Ty`）・`src/sema/typeck.rs`。名前解決の結果を前提に
式・文・関数へ型を付け、整合性を検査する。

- プリミティブ（`i8`..`u64`, `f32/f64`, `bool`, `string`, `char`, `void`）と
  ジェネリクス（`Vec<T>`, `Result<T,E>`, `Option<T>` など）を扱う
- 整数・浮動小数リテラルは未確定型として任意の整数/小数型へ適合。注釈なし `let`
  では既定型（`i32` / `f64`）へ確定
- 検出するエラー（複数件をまとめて miette で表示）:
  - 代入・引数・戻り値・`let` 注釈の型不一致
  - 算術・比較（数値）/ 論理（bool）演算の被演算子型
  - 関数呼び出しの引数個数・型
  - `!` を Result/Option 以外へ適用、または Result/Option を返さない関数内での使用
  - 不変な束縛（`let`（mut なし）/ `const`）への再代入
  - `while` の条件が bool でない、`break`/`continue` のループ外使用
- `&mut T` は `&T` として使える（参照の可変性は所有権パスで詳細検査）

#### 型定義の扱い（実装済み）

- 型名の検証（未定義の型名を報告）。既知 = プリミティブ＋組み込み（Vec/Result/Option/Box/Map/Set）＋定義済み型名
- `type` 別名は名前的型付け（別の型）。ただしリテラル代入の可否は別名の元の型で判定（`type Meters = f64` に `5.0` は可、`f64` 値は不可）
- struct: メンバアクセス `a.b` のフィールド型付け（参照は自動デリファレンス）、構造体リテラルのフィールド検査（型不一致・重複・未初期化・未知フィールド）
- enum: 定義・型名登録のみ（値の構築/分解は未対応）
- ジェネリックな型定義の本体・本体内のフィールド型は単一化未実装のため寛容（`Infer`）に扱う

### 所有権 DAG（実装済み）

`src/sema/ownership/`（`typegraph.rs` / `flow.rs` / `borrows.rs` / `mod.rs`）。仕様の
所有権 DAG（`ownership.md` / `compiler.md` の Open Question）を三層で検査する。

**1. 型レベルの所有権グラフ＋循環検出**（`typegraph.rs`）
- 型同士の所有関係を有向グラフ化し、所有のサイクル（＝無限サイズ型）を検出
- 所有辺を作らない: 参照 `&T`（非所有）、`Box`/`Vec`/`Map`/`Set`（ヒープ間接）
- 所有辺を作る（インライン格納）: 名前付き型、`Option`/`Result`/タプル/固定長配列 `T[]`、別名
- 例: `me: Bad`（直接）→ 循環エラー、`kids: Vec<Tree>` / `parent: &Tree` → OK

**2. 値レベルのムーブ＋借用グラフ（ライフタイム）**（`flow.rs`）
- 既定はムーブ。値を関数引数・`let`・`return`・構造体リテラルのフィールドへ「値として」渡すとムーブ
- `Copy` 判定: 数値プリミティブ・`bool`・`char`・不変参照 `&T`・要素が全て Copy のタプル。別名はもとの型で判定（`type Meters = f64` は Copy）
- `&x` / `&mut x` は借用（ムーブしない）。メソッド呼び出し `x.clone()` 等の受け手も借用扱い
- ムーブ済みの値の使用・借用を報告（ムーブ位置も副ラベルで表示）。再代入で再初期化。`if`/三項の分岐はムーブ集合を保守的に合流
- **ライフタイム検査**: 参照の出所（provenance）を借用グラフで追跡し、到達可能性で判定
  - ダングリング返却: ローカル／値渡し引数を指す参照を返すとエラー。参照引数経由（呼び出し側所有）は安全
  - 指す先のムーブ: 参照先がムーブされると、その参照の以降の使用をエラー

**3. 借用競合チェック（エイリアス規則）**（`borrows.rs`）
- 同一の場所に対し `&mut` は排他、`&` は複数可（`&mut` 生存中は他の借用不可）
- 借用の生存期間は**スコープベース**（保守的）: `let r = &x` の借用は宣言ブロックの終わりまで、式中の一時借用 `f(&mut x, &x)` はその文の間
- 検出: 二重 `&mut`、`&mut` と `&` の同時、同一呼び出し内の競合。別スコープ・別の場所・共有複数は OK。借用変数の再代入で解放
- 健全（実際の競合は見逃さない）だが、NLL のような最後の使用に基づく精密な生存期間ではないため保守的に多めに報告しうる

**ループ内のムーブ・借用**（`flow.rs` / `borrows.rs`）
- ムーブ: 反復をまたぐ use-after-move を検出する。本体で生じるムーブを「静かな」先行パスで入口状態に合流させてから本パスを 1 回走らせる（ムーブ集合は単調なので 2 パスで安定）。`let` の再束縛はムーブ集合をリセットするため、反復ごとに作り直す値の移動は誤検出しない
- 借用: ループ本体を独立したブロックスコープとして扱い、反復ごとに借用を解放する（スコープベースのまま）

未対応: NLL 風の精密なライフタイム領域推論（最後の使用に基づく借用終了）、参照を返却以外の長寿命の場所へ格納する一般ケース、フィールド単位の部分ムーブ（メンバの値読みはオブジェクトの借用として寛容に扱う）

### LLVM コード生成（着手・部分実装）

`src/codegen.rs`。型検査・所有権検査を通った AST を **LLVM IR のテキスト（`.ll`）** へ
落とす。この環境には `llvm-config` が無く inkwell/llvm-sys が使えないため、まずは
テキスト出力とし、`clang file.ll -o out` で実行ファイル化する（JIT は将来 LLVM 導入時）。

- 対応（確実性のため `i32` / `bool` に限定）: 関数定義・引数・再帰呼び出し、`let`/再代入/`return`、
  算術 `+ - * / %`、比較、論理 `&& ||`（短絡）、単項 `-`、`if` 文、三項演算子、
  ループ `while` / `loop` / `break` / `continue`（基本ブロック＋後方辺。`break`/`continue` はラベルスタックで解決）
- ローカルは alloca + load/store（SSA 化は LLVM の mem2reg に委ねられる）
- `main(): i32` の戻り値が終了コードになり、`clang` で実行して検証できる
- CLI: `iris --emit-llvm <file>`（IR表示）/ `iris build [--release] [-o OUT] <file>`（実行ファイル生成）/ `iris run <file>`（即実行）
- **二段ビルド**: 既定 -O0（開発・高速）、`--release` で -O2。最適化の重さがビルド時間を支配するため、開発は -O0 既定にして速くしている（800関数で約9倍差）
- `extern fn` は `declare` を出力し、`clang` が libc をリンク（`putchar` 等が使える）
- 未対応（今後）: `f64` など他の型・幅、struct/enum・メンバ/構造体リテラル、参照、
  `!`（Result/Option 伝播）、文字列、ジェネリクス

### 標準ライブラリ（最小・iris 自身で記述）

`std/std.iris`。`extern fn putchar` を借りて I/O を実現し、**iris 自身**で書いた最小の
ライブラリ。コンパイル時に各プログラムの先頭へ自動で前置される（単一の文字列として
連結し span を一意に保つ）。

- 提供: `putchar`（extern）、`put_digit` / `newline` / `print_int` / `println_int` / `println_bool`
- 現状のコード生成に合わせ `i32` / `bool` のみで記述
- これにより `main(): i32` から実際に数値・真偽値を標準出力へ表示し、`clang` でビルドして実行できる

### 未実装

- `trait` / `impl`
- enum 値の構築・分解（バリアント値の生成構文、`match` でのアンラップ）
- `match`（パターン: リテラル・範囲 `1..10`・enumアンラップ・ガード・`_`）
- ループ `for`（イテレータ）— `while` / `loop` / `break` / `continue` は実装済み
- ラムダ `(x): T -> expr`、関数型シグネチャ
- 型合成 `A + B`（ADR-0001）、関数のジェネリクス `fn f<T>(...)`
- `use`（インポート）、可視性のモジュール解決
- 文字列補間（バッククォート `` `...{expr}...` ``）
- `as` による型変換
- ブロックコメント

### バックエンド・解析（未着手）

- 借用検査の高度化（NLL 風の精密なライフタイム領域推論・部分ムーブ・ループ）— `compiler.md` の Open Question 領域
- 型推論の高度化（リテラルの後方からの確定、ジェネリクスの単一化）
- コード生成の拡張（`f64` など他の型・struct/enum・参照・文字列・I/O）、WASM ターゲット、JIT（LLVM ORC/MCJIT — 要 LLVM 導入）
- 並行処理 — `concurrency.md` 未設計

## 仕様未確定のため独自に決めた点（要確認）

実装を進めるための暫定判断。本実装前にユーザー確認のうえ `docs/spec` を更新する。

- **再代入の `mut` 位置**: `let mut x = ...`（Rust風）と仮定。docs は「mut は型修飾子」とも書くため `let x: mut T` の可能性もある。
- **代入文**: `target = value` を文として追加（docs に文法記述がなかった）。
- **`else`**: `}` と同じ行に必要（改行をまたぐ `else` は未対応）。

## ビルド・実行

```sh
cargo build
cargo test
cargo run -- examples/hello.iris              # AST を表示

# 実行ファイルを生成・実行（i32/bool の部分集合のみ）
cargo run -- run examples/print.iris          # 生成して即実行（既定 -O0）
cargo run -- build examples/print.iris        # 実行ファイルを生成（既定 -O0=高速）
cargo run -- build --release -o prog examples/print.iris   # -O2（最適化）
cargo run -- --emit-llvm examples/print.iris  # LLVM IR を表示
```

ビルドは既定 **-O0**（開発用・高速）、`--release` で **-O2**。リンク/実行ファイル化は
`clang` に委ねる。計測例（800関数）: `-O0` 156ms vs `-O2` 1468ms（約9倍差）。最適化の
重さがビルド時間を支配するため、開発ループは -O0 既定で速い。

### 速度検証（ベンチ）

再現可能なベンチを `examples/` に用意（リリースで実行）:

```sh
cargo run --release --example bench_compile     # コンパイル速度（フロント throughput と -O0/-O1/-O2）
cargo run --release --example bench_ownership   # 所有権 DAG（型グラフ循環検出・借用グラフ）の速度
```

計測の要点:
- **フロントエンド＋コード生成**（`compile_ir`、clang 除く）は概ね線形で **~20〜40万行/秒**（in-process 計測）。
- **ビルド総時間は clang の最適化レベルが支配**: 1000関数で -O0 ≈145ms / -O1 ≈1736ms / -O2 ≈1643ms（-O0→-O1 で約11倍）。→ 開発は -O0 既定が効く。
- 所有権解析はマイクロ〜ミリ秒オーダーで、ボトルネックにならない（借用競合チェックは生存中の借用を場所ごとに索引し、借用数に対して線形 O(M)）。
