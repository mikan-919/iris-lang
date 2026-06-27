# Plan 002: 実用規模の iris プログラムを 1 本書き、機能の合成を実証する統合テストにする

> **Executor instructions**: この計画を上から順に実行する。各ステップの検証コマンドを
> 実行し、期待結果を確認してから次へ進む。「STOP conditions」に該当したら、改善を試みず
> 停止して報告する。完了したら `plans/README.md` の本計画の行を更新する。
>
> **Drift check (run first)**: `git diff --stat 6512ede..HEAD -- std/ src/ examples/ tests/`
> in-scope のファイルが計画作成後に変わっていたら、下の「Current state」抜粋と実コードを
> 突き合わせ、食い違えば STOP 条件として扱う。

## Status

- **Priority**: P1
- **Effort**: S（設計は確定・検証済み。残りは「書く・テスト追加・全テスト緑」）
- **Risk**: LOW
- **Depends on**: none（007・008 とも解決済み）
- **Category**: direction / tests
- **Planned at**: commit `9ffa348`, 2026-06-25。**Revised**: commit `6512ede`, 2026-06-26
  （初回 RPN 設計が言語機能不足で論理破綻していたため再設計。下記「Revision の経緯」参照）

## Revision の経緯（重要・必読）

初回の `examples/showcase.iris`（RPN 電卓、commit `f18aa43` で追加）は **論理的に正しく
動かない**ことが判明した。設計が言語の未実装機能を必要としていたためで、これは本プランが
狙っていた「機能の継ぎ目の発見」そのものである:

- RPN は**ポップ可能なスタック**を要する。実装は `Stack.push` が `Vec.push`（末尾追加のみ）、
  `Stack.pop` が手動カウンタ `top` を減らして `data[top]` を読む方式だった。
- ところが **`Vec.pop` は未実装**（STATUS.md:246,345）、**索引代入 `v[i] = x` も codegen
  未対応**（実測: `cargo run -- run` でエラー「この代入先はコード生成に未対応です」）。
- このため一度でも pop→push すると `top` と Vec の実長が乖離し、`pop` が古いスロットを読む。
  `"3 4 + 5 *"` は **35 ではなく 3** を返した（終了コードで実測確認）。

**結論**: 現行機能では縮む／上書きできるスタックは書けない → 一般の RPN 評価器は正しく
書けない。そこで設計を **append-only な Vec で正しく書ける左結合電卓**に変更する（下記）。
6 機能カテゴリは引き続き全部踏む。この未実装機能（`Vec.pop` / 索引代入）の追加は別プランの
仕事（README の「計画化候補」に記録）。

## Why this matters

iris は機能ごとに回帰テストが揃っているが（`tests/*.rs`）、**それらが 1 本の現実的な
プログラムの中で合成できるか**を確かめたものが無い。実用規模のプログラムを 1 本通すと、
単体テストでは出ない「機能の継ぎ目のバグ」が安価に表面化し（実際 上記の通り表面化した）、
同時に「広まってほしい」（CONCEPT.md）言語の看板デモにもなる。

## Current state

- `examples/showcase.iris` — **既存。RPN 版で論理破綻**（上記）。本プランで**全置換**する。
- `tests/codegen.rs` — IR 検査＋clang 実走テスト。実走ヘルパは終了コードを返す:
  ```rust
  // tests/codegen.rs:21
  /// IR を clang でコンパイルして実行し、終了コードを返す。clang が無ければ None。
  fn run_exit_code(src: &str, tag: &str) -> Option<i32> { ... }
  // 構造の手本: tests/codegen.rs:135 runs_factorial
  ```
  showcase 用の実走テストは**まだ無い**（追加するのが Step 2）。
- 利用可能機能（STATUS.md・すべて clang 実走済み）: struct・enum（ジェネリック含む）・
  `Vec<T>`（`len`/`push`・**末尾追加のみ**）・`match`（束縛・ガード・範囲・or・網羅性・catch-all）・
  trait＋境界・固有メソッド `impl Type`・`!` 伝播・`for`・`as` 変換・文字列 `s.len()`/`s[i]`
  （→`u8`）/`a.concat(b)`。
- **使えない**（設計時に避けること）: `Vec.pop`・索引代入 `v[i]=x`・非 Copy 要素の `Vec.push`/反復・
  スライス・クロージャ・文字列補間。enum は move-only（ADR-0005）なので `&mut self` 経由で
  enum フィールドを値として読むとムーブになる（→演算子は Copy な `i32` コードで保持する）。
- 設計値（CONCEPT.md）: 改行が文区切り（`;` 禁止）、`mut` は宣言側、コメントは日本語。

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| ビルド | `cargo build` | exit 0 |
| showcase を実行 | `cargo run -- run examples/showcase.iris` | プログラムが走り、直後の `echo $?` が **35** |
| IR 表示（デバッグ用） | `cargo run -- --emit-llvm examples/showcase.iris` | LLVM IR テキスト |
| 統合テスト | `cargo test --test codegen runs_showcase_program` | 1 passed |
| 全テスト | `cargo test` | 既存テストが緑のまま |

## Scope

**In scope**:
- `examples/showcase.iris`（既存ファイルを**下記の検証済みソースで全置換**）
- `tests/codegen.rs`（末尾に実走テストを 1 件追加）

**Out of scope**（関係しそうでも触らない）:
- `src/` のコンパイラ本体 — このプランは**コンパイラを変更しない**。
- `std/` — 既存 std で書ける。
- `examples/showcase.md` 等の追加ドキュメントは任意（やらなくてよい）。

## Git workflow

- Branch: ブランチ名は任意（例 `advisor/002-showcase`）。worktree 内で作業。
- コミットは論理単位。メッセージは repo 準拠の Conventional Commits（日本語可）。
- 指示が無ければ push / PR はしない。

## Steps

### Step 1: `examples/showcase.iris` を下記ソースで全置換する

`examples/showcase.iris` の中身を**以下のソースで完全に置き換える**（このソースは
`cargo run -- run` で exit 35 を返すことを検証済み）。一字一句そのまま書くこと。

```iris
// showcase.iris — 左結合の簡易電卓（演算子優先順位なし）
//
// 仕様:
//   ソース内のリテラル式 "3+4*5" を左から右へ評価し、結果を main の終了コードで返す。
//   優先順位は無視し左結合: ((3 + 4) * 5) = 35。
//
// 踏む機能:
//   1. string — s[i]（→ u8）、s.len()、as i32 変換
//   2. Vec<i32> + struct — 計算履歴 log を append-only で積む（push / len / 添字読み）
//   3. enum（Token）+ match — Some(tok) 束縛・catch-all・Num(d)/Add/Sub/Mul の網羅
//   4. impl（Calc の固有メソッド feed / set_op / result）
//   5. ! エラー伝播 — digit_token が byte_to_digit の Option<i32> を ! で連鎖
//   6. for x in lo..hi — 文字列を 1 バイト単位でスキャン
//
// 終了コードの期待値: 35
//
// 設計メモ:
//   Vec.pop も索引代入 v[i]=x も未実装のため、ポップ可能なスタックは書けない。
//   そこで acc を持つ電卓を append-only な Vec で実装する（log は履歴・縮まない）。

// ---- トークン型 -------------------------------------------------------

type Token = enum {
    Num(i32)
    Add
    Sub
    Mul
}

// ---- 電卓状態（acc + 直前の演算子コード + 履歴 Vec） ------------------
//
// op は i32 コード（0=Add, 1=Sub, 2=Mul）。enum を &mut self 経由で
// 値として読むとムーブになるため、演算子は Copy な i32 で保持する。

type Calc = struct {
    acc: i32
    op: i32
    log: Vec<i32>
}

// 直前の演算子 op を acc に対し被演算子 d で適用した結果を返す。
fn op_apply(acc: i32, op: i32, d: i32): i32 {
    if op == 1 {
        return acc - d
    }
    if op == 2 {
        return acc * d
    }
    return acc + d
}

impl Calc {
    // 数 d を直前の演算子で acc に適用し、結果を履歴に積む。
    fn feed(&mut self, d: i32): i32 {
        self.acc = op_apply(self.acc, self.op, d)
        self.log.push(self.acc)
        return 0
    }

    // 次に適用する演算子コードを設定する。
    fn set_op(&mut self, o: i32): i32 {
        self.op = o
        return 0
    }

    // 履歴の最後の値（= 最終結果）を返す。
    fn result(&self): i32 {
        let n = self.log.len() as i32
        return self.log[n - 1]
    }
}

// ---- 1 バイトの数字解析 -----------------------------------------------

fn byte_to_digit(b: u8): Option<i32> {
    let n = b as i32
    if n >= 48 && n <= 57 {
        return Some(n - 48)
    }
    return None
}

// ---- 数字バイトを Num トークンに変換する（! 伝播のデモ）--------------

fn digit_token(b: u8): Option<Token> {
    let d = byte_to_digit(b)!
    return Some(Num(d))
}

// ---- 1 バイトのトークン解析 -------------------------------------------

fn parse_token(b: u8): Option<Token> {
    let c = b as i32
    if c >= 48 && c <= 57 {
        return digit_token(b)
    }
    if c == 43 {
        return Some(Add)
    }
    if c == 45 {
        return Some(Sub)
    }
    if c == 42 {
        return Some(Mul)
    }
    return None
}

// ---- 1 トークンを電卓に適用する ---------------------------------------
//
// match の各アームは単一式（メソッド呼び出し）。数なら feed、
// 演算子なら set_op を呼ぶ。

fn step(calc: &mut Calc, tok: Token): i32 {
    match tok {
        Num(d) -> calc.feed(d)
        Add -> calc.set_op(0)
        Sub -> calc.set_op(1)
        Mul -> calc.set_op(2)
    }
}

// ---- 評価 -------------------------------------------------------------

fn eval(expr: string): i32 {
    let mut calc = Calc { acc: 0, op: 0, log: [] }
    let n = expr.len()
    for i in 0..n {
        let b = expr[i]
        let tok = parse_token(b)
        match tok {
            Some(t) -> step(&mut calc, t)
            _ -> 0
        }
    }
    return calc.result()
}

// ---- エントリポイント -------------------------------------------------

fn main(): i32 {
    let expr = "3+4*5"
    return eval(expr)
}
```

**Verify**: `cargo run -- run examples/showcase.iris` が走り、直後の `echo $?` が **35**。
（35 以外、またはコンパイルエラーが出たら STOP 条件へ。場当たり修正をしない。）

### Step 2: 統合テストを 1 件追加する

`tests/codegen.rs` の**末尾**に、`runs_factorial`（`tests/codegen.rs:135`）と同じ構造で
実走テストを追加する。`examples/showcase.iris` を `include_str!` で読み込み、
`run_exit_code` に渡して終了コードを検証する:

```rust
#[test]
fn runs_showcase_program() {
    // examples/showcase.iris が 6 機能カテゴリを合成して 35 を返すことの実走回帰。
    // （string索引 / Vec<i32>+struct / enum+match / impl / !伝播 / for）
    let src = include_str!("../examples/showcase.iris");
    if let Some(code) = run_exit_code(src, "showcase") {
        assert_eq!(code, 35);
    }
}
```
（`run_exit_code` は clang 不在なら `None` を返すので、`if let Some` で包むのが既存流儀。）

**Verify**: `cargo test --test codegen runs_showcase_program` → `1 passed`。

### Step 3: 回帰がないことを確認する

**Verify**: `cargo test` → 既存テストが緑のまま、新規 1 件 pass。`git status` で
in-scope（`examples/showcase.iris`・`tests/codegen.rs`）以外を変更していないこと。

## Test plan

- 新規テスト: `tests/codegen.rs::runs_showcase_program` — happy path 実走、exit 35 を検証。
  clang 不在環境はスキップ（既存 `run_exit_code` 流儀）。
- 構造の手本: `tests/codegen.rs:135 runs_factorial`。
- 検証: `cargo test` → 既存緑のまま＋新規 1 件 pass。

## Done criteria

ALL must hold:

- [ ] `examples/showcase.iris` が Step 1 の検証済みソースで置き換わっている
- [ ] `cargo run -- run examples/showcase.iris` 実行直後の `echo $?` が **35**
- [ ] `tests/codegen.rs` に `runs_showcase_program` を追加、`cargo test --test codegen runs_showcase_program` が緑
- [ ] `cargo test` 全体が緑（既存に回帰なし）
- [ ] in-scope 外のファイルを変更していない（`git status`）

## STOP conditions

改善を試みず停止して報告する:

- Step 1 のソースがコンパイル/実行に失敗、または exit が 35 でない
  → 「Current state」抜粋（`run_exit_code` の形・利用可能機能）が実コードと食い違っている
    可能性。最小再現・エラーを添えて報告（**直さない**）。
- プログラムを通すために `src/` か `std/` の変更が必要に見える → 報告（別プランの仕事）。

## Maintenance notes

- このプログラムは「実装済み機能の合成」の生きた回帰になる。今後 codegen を触ったら
  `cargo test --test codegen runs_showcase_program` が壊れていないか見る。
- `Vec.pop` / 索引代入 `v[i]=x` が将来入ったら、本来やりたかった RPN スタック版 showcase を
  第 2 サンプルとして足すと良い（README「計画化候補」参照）。
- 将来クロージャ（plan 003）や文字列補間（plan 004）が入ったら、それらを踏む showcase を足す。
