# Plan 005: 読み取り専用の libc 文字列関数（strlen/strcmp/strncmp）を iris 実装に置き換え、既定経路を libc 非依存にする

> **Executor instructions**: 上から順に。各ステップの検証コマンドを実行し期待結果を確認
> してから次へ。「STOP conditions」に該当したら停止して報告。完了後 `plans/README.md` を更新。
>
> **Drift check (run first)**: `git diff --stat 9ffa348..HEAD -- std/prelude.iris src/codegen.rs`
> in-scope が変わっていたら下の抜粋と実コードを突き合わせ、ずれていれば STOP。

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: direction（self-contained ランタイム・ADR-0011/0012 の完遂）
- **Planned at**: commit `9ffa348`, 2026-06-25

## Why this matters

ADR-0011/0012 は「生成物から libc 依存を外す」中間目標で、syscall 原語・mmap アロケータ・
自前 `_start` まで実装済み。しかし**実態はまだ libc 非依存になっていない**:
`std/prelude.iris`（全プログラムに自動前置）に 8 個の libc extern が残り（`strcmp` `strlen`
`strncmp` `malloc` `free` `memset` `strcpy` `strcat`）、ごく普通の `puts`／`s.len()`／文字列
`match` を使うだけで `strlen`/`strcmp` 経由で libc がリンクされる。本プランは、そのうち
**読み取り専用で iris に書き直せる 3 つ（`strlen`/`strcmp`/`strncmp`）**を iris のバイト
ループへ置き換え、`puts`・`.len()`・文字列 `match` の既定経路を真に libc 非依存にする。

**重要な線引き（先に読む）**: `strcpy`/`strcat`/`memset`/`malloc`/`free` はバッファへの
**バイト書き込み**を要するが、iris は**索引代入 `buf[i] = v` を codegen 未対応**
（`src/codegen.rs:1693` の代入先生成が ident/フィールド/参照越しのみ。Index は
「この代入先はコード生成に未対応です」で弾く）。よってこれらの iris 実装には**索引ストアの
codegen 追加が前提**になり、本プランの対象外（別プラン）。本プランは「前提なしで今できる
読み取り系」だけを刈り取る。

## Current state

- 残存 libc extern（`std/prelude.iris:20-30`）:
  ```iris
  extern fn strcmp(a: string, b: string): i32        // 行21  — match 文字列パターン比較で使用
  extern fn strlen(s: string): i32                   // 行24  — string.len()・puts で使用
  extern fn strncmp(a: string, b: string, n: i64): i32 // 行25 — os.iris getenv で使用
  extern fn malloc(n: i64): RawPtr                   // 行26  — string.concat・LibcAlloc
  extern fn free(p: RawPtr): void                    // 行27  — LibcAlloc
  extern fn memset(ptr: RawPtr, val: i32, n: i64): RawPtr // 行28 — os.iris getenv
  extern fn strcpy(dst: string, src: string): string // 行29  — string.concat
  extern fn strcat(dst: string, src: string): string // 行30  — string.concat
  ```
- `strcmp` は codegen が文字列 `match` で直接呼ぶ:
  ```rust
  // src/codegen.rs:2427
  self.emit(&format!("{r} = call i32 @strcmp(ptr {scrut_val}, ptr {gref})"));
  ```
  iris で `fn strcmp(...): i32` を書くと `define i32 @strcmp(ptr, ptr)` が emit され、この
  `call @strcmp` がそれに解決される（外部 libc シンボルが不要になる）。
- 文字列は NUL 終端・`ptr` 表現。索引 `s[i]` は i バイト目を `u8` で読む（load、実装済み）。
  `as` 変換で `s[i] as i32` 等が書ける（STATUS）。`==`/`<` 等の比較・算術は実装済み。
- `for x in lo..hi`・`while`・`loop`・`if`・再帰すべて使える。
- `@__iris_alloc`/`@__iris_free` は codegen 生成の inline-asm（mmap/munmap）で**既に libc 非
  依存**（`src/codegen.rs:600,613`）。Vec はこれを使う。
- 索引代入は未対応（上記。`strcpy` 等の前提）。

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| ビルド | `cargo build` | exit 0 |
| codegen 実走 | `cargo test --test codegen` | 緑（既存回帰なし） |
| libc 非依存リンク確認 | 下記 Step 3 のワンライナー | `-nostdlib` でリンク・実走できる |
| 全テスト | `cargo test` | 緑 |

## Scope

**In scope**:
- `std/prelude.iris` — `strlen`/`strcmp`/`strncmp` の `extern` を iris 実装に置換
- `tests/codegen.rs` — iris 実装版が正しく動く実走テスト＋ libc 非依存リンクの検証
- `docs/STATUS.md` — 残存 libc の表を更新

**Out of scope**（索引ストア codegen が前提・別プラン）:
- `strcpy`/`strcat`/`memset`/`malloc`/`free` — バッファ書き込みを要する。**触らない**。
  これらが残るため `string.concat`／`os.getenv` は当面 libc に残る（健全）。
- `std/alloc.iris` の `LibcAlloc` — **設計上 libc を包む実装**（ADR-0012）。残すのが正しい。
- 索引代入 `buf[i] = v` の codegen 追加 — 別プランの仕事（本プランの前提を外す作業）。
- `src/codegen.rs:2427` の `@strcmp` 呼び出し箇所 — 変えない（シンボル名は据え置き、定義側
  だけ iris に移す）。

## Git workflow

- Branch: `advisor/004-drop-libc-readonly`
- 関数ごと or 論理単位でコミット。メッセージは repo 準拠
  （例 `feat(std): strlen/strcmp/strncmp を iris 実装へ移行（libc 非依存）`）。
- push / PR は指示が無ければしない。

## Steps

### Step 1: strlen / strcmp / strncmp を iris で書く

`std/prelude.iris:24,21,25` の 3 つの `extern` を削除し、iris 実装に置き換える。NUL 終端の
バイト列を `s[i]`（→`u8`）で走査する。例（シグネチャは既存 extern と一致させること。codegen
の `@strcmp` 呼び出しが `i32` 戻り・`ptr` 引数を期待する点に注意）:

```iris
// NUL 終端文字列の長さ（libc 非依存・iris 実装）。
fn strlen(s: string): i32 {
    let mut i = 0
    while s[i] as i32 != 0 {
        i = i + 1
    }
    return i
}

// NUL 終端文字列を辞書順比較。等しければ 0、a<b で負、a>b で正（libc strcmp 互換）。
fn strcmp(a: string, b: string): i32 {
    let mut i = 0
    while a[i] as i32 != 0 {
        if a[i] as i32 != b[i] as i32 {
            return a[i] as i32 - b[i] as i32
        }
        i = i + 1
    }
    return 0 - (b[i] as i32)   // a が尽きた。b が長ければ負を返す
}
// strncmp も同様に n バイトまで比較（引数 n: i64）
```
（`match` 文字列比較は等価のみ使うので `strcmp` は「0 か否か」だけ正しければ機能上十分だが、
libc 互換の符号も上のように容易に満たせる。`while` 条件・`as` 変換・`s[i]` は実装済み機能。）

**Verify**: `cargo build` が exit 0。`std/prelude.iris` に `extern fn strlen`/`strcmp`/`strncmp`
が**無い**こと（`grep -n "extern fn strlen\|extern fn strcmp\|extern fn strncmp" std/prelude.iris`
が空）。

### Step 2: 既存の文字列テストが緑であることを確認する

`tests/codegen.rs` の既存テスト（`runs_string_concat` 等・文字列 `match`・`s.len()`）を実走
して回帰が無いことを確認する。`s.len()` は新 `strlen` を呼ぶようになる。

**Verify**: `cargo test --test codegen` が緑。特に `s.len()` を使うテストと文字列 `match` の
テストが pass。

### Step 3: libc 非依存リンクを実走で固定する（新規テスト）

`tests/codegen.rs` に、**`puts` ＋ `s.len()` ＋ 文字列 `match`** を使い `concat`/`getenv` を
**使わない**プログラムを、libc 抜き（`clang -nostartfiles -nostdlib`）でリンク・実走できる
ことを検証するテストを 1 件足す。既存 `run_exit_code`（`tests/codegen.rs:21`、`-nostartfiles`
のみ）を手本に、`-nostdlib` も付けた変種ヘルパ `run_exit_code_nostdlib` を作る（clang 不在は
`None` でスキップ）。

```rust
#[test]
fn runs_without_libc_for_string_readonly_path() {
    // puts / s.len() / 文字列 match だけなら libc 無し（-nostdlib）でリンク・実走できる。
    let src = "fn main(): i32 {\n    let s = \"hi\"\n    let n = s.len()\n    let k = match s {\n        \"hi\" -> 7\n        _ -> 0\n    }\n    return n + k\n}"; // 2 + 7 = 9
    if let Some(code) = run_exit_code_nostdlib(src, "nolibc_str") {
        assert_eq!(code, 9);
    }
}
```

**Verify**: `cargo test --test codegen runs_without_libc_for_string_readonly_path` が緑。
（`-nostdlib` でリンクが通る＝この経路に libc 呼び出しが残っていない証明。）

### Step 4: STATUS を更新する

`docs/STATUS.md` の「標準ライブラリ」「self-contained ランタイム」節を更新: `strlen`/`strcmp`/
`strncmp` は iris 実装で libc 非依存になったこと、残りの `strcpy`/`strcat`/`memset`/`malloc`/
`free` は**索引代入 `buf[i] = v` の codegen 未対応が前提**のため libc に残る（別プラン）こと
を明記する。

**Verify**: `grep -n "索引代入\|buf\[i\]\|libc 非依存" docs/STATUS.md` が更新内容を含む。

## Test plan

- 新規: `tests/codegen.rs::runs_without_libc_for_string_readonly_path` — `-nostdlib` 実走で
  libc 非依存を固定（手本: `run_exit_code` `tests/codegen.rs:21`）。
- 回帰: 既存の `s.len()`／文字列 `match`／`runs_string_concat` が緑のまま（新 strlen/strcmp
  に差し替わっても挙動不変）。
- 検証: `cargo test` 全体が緑。

## Done criteria

ALL must hold:

- [ ] `std/prelude.iris` に `extern fn strlen`/`strcmp`/`strncmp` が無い（`grep` で空）
- [ ] 3 関数の iris 実装が存在し、既存の文字列テストが緑
- [ ] `runs_without_libc_for_string_readonly_path` が `-nostdlib` で実走 pass
- [ ] `strcpy`/`strcat`/`memset`/`malloc`/`free`・`LibcAlloc`・`src/codegen.rs` を変更して
      いない（`git diff --stat`）
- [ ] `docs/STATUS.md` を更新（残存 libc と前提を明記）
- [ ] in-scope 外を変更していない（`git status`）
- [ ] `plans/README.md` の本計画の行を更新

## STOP conditions

停止して報告する:

- `s[i]` の読み・`while`・`as` 変換のいずれかが期待通り動かず iris 実装が書けない
  → 最小再現を添えて報告（実装済みのはずの機能の不具合＝発見）。
- 新 `strcmp`（iris の `@strcmp`）が codegen の `call @strcmp`（`src/codegen.rs:2427`）と
  シンボル/シグネチャで噛み合わず、文字列 `match` がリンク/実走で壊れる → 報告。
- `-nostdlib` リンクが、`puts`/`len`/`match` だけのプログラムでも通らない（想定外の libc
  依存が残っている）→ 何が未解決シンボルかを `clang` 出力ごと報告。
- `std/prelude.iris:20-30` の抜粋が実ファイルと食い違う。

## Maintenance notes

- 残りの libc 排除（`strcpy`/`strcat`/`memset` → iris、`concat` の `malloc` → mmap）は
  **索引代入 `buf[i] = v` の codegen 追加**が前提。これを別プランで先に入れると、`concat`/
  `getenv` も iris 化でき libc 完全排除に到達する。本プランの STATUS 追記にこの依存を残す。
- `LibcAlloc`（`std/alloc.iris`）は**意図的な libc ラッパ**（ADR-0012）。libc を消す対象では
  ない。既定経路は `MmapAlloc` 相当の `@__iris_alloc`。
- レビュアは「`strcmp` のシンボル名が codegen の呼び出しと一致しているか」「`-nostdlib`
  テストが本当に libc 非依存を保証しているか」を見る。
