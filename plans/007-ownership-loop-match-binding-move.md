# Plan 007: ループ内 match アーム束縛の偽 use-after-move を修正する

> **Executor instructions**: この計画を上から順に実行する。各ステップの検証コマンドを
> 実行し、期待結果を確認してから次へ進む。「STOP conditions」に該当したら、改善を試みず
> 停止して報告する。完了したら自分の作業ブランチにコミットする（`plans/README.md` の更新は
> レビュア側で行う）。
>
> **Drift check (run first)**: `git diff --stat f18aa43..HEAD -- src/sema/ownership/`
> 出力があれば、下の「Current state」抜粋と実コードを突き合わせ、食い違えば STOP。

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none（plan 002 が発見したバグの修正。002 のブロック解除につながる）
- **Category**: correctness / soundness
- **Planned at**: commit `f18aa43`, 2026-06-26

## Why this matters

`for`/`while`/`loop` の本体に `match` があり、その**アームのパターン束縛変数**（`Some(t) ->`
の `t` など）を**ムーブ**で消費すると、所有権検査が**偽の use-after-move エラー**を出して
コンパイルが落ちる。健全（unsound ではない）だが、**正しいプログラムを誤って拒否する**完全性
（completeness）のバグ。plan 002 の showcase（RPN 電卓）がこれに当たって実走に到達できなかった。

ループ本体は反復間の use-after-move を検出するため「先行（probe）パス → 入口へ合流 → 本パス」
の 2 パスで解析する（`flow.rs:240-251`）。probe パスで `t` がムーブされると、そのムーブ集合が
入口状態へ合流し、本パスの match アームに入った時点で `t` が**既にムーブ済み**として残る。
ところが match アームのパターン束縛は反復ごとに**新しく束縛し直される**ので、アーム本体を見る
前にその束縛変数のムーブ状態をクリアしなければならない。`for`/`for-in` のループ変数は既に
これをやっている（`flow.rs:212-216, 227-231` の `st.moved.remove(&id)`）が、**match アームの
束縛変数には同じリセットが無い**のが根本原因。

### 確認済みの再現（このプランの根拠）

ループありで失敗・ループなしで成功する、という挙動の差がバグの証拠:

```iris
type Token = enum {
    Num(i32)
    Add
}
type Stack = struct { top: i32 }
fn apply(st: &mut Stack, t: Token): i32 { return 0 }
fn main(): i32 {
    let mut st = Stack { top: 0 }
    for i in 0..3 {
        let o: Option<Token> = Some(Num(i))
        match o {
            Some(t) -> apply(&mut st, t)   // ← 偽の use-after-move
            _ -> 0
        }
    }
    return 0
}
```

- `for` あり: `所有権検査に失敗しました（1 件）… ムーブ済みの値 `t` を使用しています` で exit 1。
- `for` を外して match を 1 回だけ実行: **正常に通る**（exit 0）。

## Current state

`src/sema/ownership/flow.rs` の `ExprKind::Match` ハンドラ（行番号は `f18aa43` 時点）:

```rust
// flow.rs:346
ExprKind::Match { scrutinee, arms } => {
    // scrutinee を消費する（move）。
    self.visit_expr(scrutinee, st);
    // 各アームは独立した分岐（if と同じ保守的な合流）。
    let mut merged = st.clone();
    // 全アームのムーブ集合を合流させる。
    let arm_states: Vec<State> = arms
        .iter()
        .map(|arm| {
            let mut arm_st = st.clone();
            // ガードは本体より先に評価される（条件＝読み取り）。
            if let Some(g) = &arm.guard {
                self.visit_operand(g, &mut arm_st);
            }
            self.visit_expr(&arm.body, &mut arm_st);
            arm_st
        })
        .collect();
    for arm_st in arm_states {
        merged = merge(merged, arm_st);
    }
    *st = merged;
}
```

ここにはパターン束縛のムーブ状態リセットが**無い**。対して、ループ変数は持っている手本:

```rust
// flow.rs:212（Stmt::For）／ 227（Stmt::ForIn）も同型
// ループ変数は反復ごとに再束縛される整数（Copy）。念のため状態をリセット。
if let Some(&id) = self.def_spans.get(var_span) {
    st.moved.remove(&id);
    st.dangling.remove(&id);
}
```

参照する型（`src/ast.rs`）:

```rust
// ast.rs:329
pub enum Pattern {
    Wildcard { span: Span },
    Lit { value: LitPat, span: Span },
    Range { lo: LitPat, hi: LitPat, inclusive: bool, span: Span },
    /// `VariantName(binding)` — ペイロード束縛付きバリアント。
    Variant {
        name: String,
        /// ペイロードを束縛する変数名と宣言 span。
        binding: (String, Span),
        span: Span,
    },
    /// `SomeName` — 素の識別子パターン。typeck でバリアント名か束縛変数かを判定する。
    Bind { name: String, span: Span },
    /// `pat | pat | ...` — or パターン。
    Or { patterns: Vec<Pattern>, span: Span },
}
```

`def_spans: HashMap<Span, DefId>`（`flow.rs:127`）は**宣言 span → DefId**。束縛変数の宣言 span を
キーに引ける（`Pattern::Variant` は `binding.1`、`Pattern::Bind` は `span`）。バリアント名だけの
`Bind`（束縛でない）は `def_spans` に載らないので `get` が `None` を返すだけで無害。

## なぜこの修正で十分か（健全性）

パターン束縛変数はそのアームで**初めて生成される**ので、束縛より前にそれを使う経路は存在しない。
したがってアーム入口でその DefId のムーブ状態をクリアしても、**本物の** use-after-move を見逃す
ことはない（ループ変数のリセットと同じ理屈）。クリアはガードより前に行う（ガードも束縛を読める）。

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| ビルド | `cargo build` | exit 0 |
| 再現（修正前は失敗） | `cargo run -q -- --emit-llvm <repro>.iris` | 修正後は exit 0・IR 出力 |
| 所有権テスト | `cargo test --test ownership` | 緑（新規 1 件含む） |
| 全テスト | `cargo test` | 既存緑のまま＋新規 pass |

> 注: テストファイル名は実在を確認すること（`ls tests/`）。所有権の回帰が
> `tests/ownership.rs` でなければ、実在する所有権テストファイルに合わせる。

## Scope

**In scope**:
- `src/sema/ownership/flow.rs`（`ExprKind::Match` アームに束縛リセットを追加）
- 所有権の回帰テスト 1 件（`tests/ownership.rs` 等、既存の所有権テストファイル末尾）

**Out of scope**（触らない）:
- `src/ast.rs`・パーサ・typeck・resolve — AST もパイプラインも変えない。
- `examples/showcase.iris` — plan 002 の成果物。**このプランでは触らない**（修正が入れば
  002 を再実行して通す。それは 002 側の仕事）。
- `match` の網羅性・合流ロジック本体 — 束縛リセットの 1 点だけを足す。

## Git workflow

- Branch: `fix/ownership-loop-match-binding`（`f18aa43` から分岐）
- メッセージは repo 準拠の Conventional Commits（日本語可）。例:
  `fix(ownership): ループ内 match アーム束縛の偽 use-after-move を修正`
- 指示が無ければ push / PR はしない。

## Steps

### Step 1: 再現を 1 本用意して赤を確認する

上の「確認済みの再現」を `/tmp/repro_match_move.iris` に保存し、
`cargo run -q -- --emit-llvm /tmp/repro_match_move.iris` を実行。
**修正前は** `所有権検査に失敗しました` で exit 1 になることを確認（赤の確認）。

**Verify**: exit 1・`ムーブ済みの値 `t` を使用しています` が出る。

### Step 2: match アームでパターン束縛のムーブ状態をクリアする

`flow.rs:346` の `ExprKind::Match` アームの `map` クロージャ内、`arm_st` を clone した直後・
**ガード評価より前**に、`arm.pattern` の束縛変数を `arm_st.moved` / `arm_st.dangling` から
取り除く。束縛 span の収集はループ変数のリセット（`flow.rs:212`）と同型で書く。

実装方針（束縛 span を集める小さなヘルパを `impl` 内に追加し、再帰で `Or` も拾う）:

```rust
// パターンが束縛する変数の宣言 span を集める（Variant のペイロード・素の Bind・
// Or の各選択肢）。バリアント名だけの Bind は def_spans に載らないので無害。
fn pattern_binding_spans(pat: &Pattern, out: &mut Vec<Span>) {
    match pat {
        Pattern::Variant { binding, .. } => out.push(binding.1),
        Pattern::Bind { span, .. } => out.push(*span),
        Pattern::Or { patterns, .. } => {
            for p in patterns {
                pattern_binding_spans(p, out);
            }
        }
        Pattern::Wildcard { .. } | Pattern::Lit { .. } | Pattern::Range { .. } => {}
    }
}
```

`map` クロージャ内（`let mut arm_st = st.clone();` の直後）:

```rust
let mut arm_st = st.clone();
// パターン束縛はこのアームで新たに束縛し直されるので、（ループの先行パスで付いた）
// 古いムーブ状態をクリアする。ループ変数のリセット（上の For 参照）と同じ理屈。
let mut binds = Vec::new();
Self::pattern_binding_spans(&arm.pattern, &mut binds);
for span in binds {
    if let Some(&id) = self.def_spans.get(&span) {
        arm_st.moved.remove(&id);
        arm_st.dangling.remove(&id);
    }
}
if let Some(g) = &arm.guard { /* 既存のまま */ }
```

> `pattern_binding_spans` は `&self` を取らない関連関数なので `Self::` で呼ぶ（借用衝突を避ける）。
> `Span` が `Copy` でなければ `binding.1.clone()` / `*span` を調整する（`def_spans` のキーが
> `Span` なので `Copy` のはず。ビルドエラーが出たら型に合わせる）。

**Verify**: `cargo build` が exit 0。

### Step 3: 再現が緑になることを確認する

`cargo run -q -- --emit-llvm /tmp/repro_match_move.iris` が **exit 0** で IR を出すこと。

**Verify**: exit 0。所有権エラーが消えている。

### Step 4: 回帰テストを 1 件追加する

既存の所有権テストファイル（`ls tests/` で確認。おそらく `tests/ownership.rs`）の末尾に、
ループ内 match 束縛のムーブが**通る**ことを検証するテストを 1 件追加する。既存テストの
ヘルパ（その場で `analyze` を呼びエラー件数 0 を確認する流儀など）に**合わせる**こと。
本物の use-after-move が見逃されていないことの対も 1 つ入れると理想（例: 同じ束縛を
match の外で 2 回ムーブする等、別アームではなく単純な二重ムーブ）。ただし最低限は
「ループ内 match 束縛ムーブがエラー 0 件で通る」1 件で可。

**Verify**: `cargo test --test ownership` が緑（新規含む）。

### Step 5: 全テスト緑を確認する

**Verify**: `cargo test` が全緑。既存に回帰なし。特に `tests/codegen.rs`・`tests/typeck.rs`・
match 系のテストが落ちていないこと。

## Test plan

- 新規: ループ内 `match` アーム束縛をムーブで消費するプログラムがエラー 0 件で通る回帰
  （`tests/ownership.rs` 等の既存流儀に合わせる）。
- 既存の match／所有権テストが全緑のまま（合流ロジックを壊していないことの担保）。

## Done criteria

ALL must hold:

- [ ] Step 1 の再現が修正後 `cargo run -q -- --emit-llvm` で exit 0
- [ ] `flow.rs` の `ExprKind::Match` でパターン束縛のムーブ／dangling 状態をクリアしている
- [ ] 所有権テストに回帰 1 件、`cargo test --test ownership` が緑
- [ ] `cargo test` 全体が緑（既存に回帰なし）
- [ ] in-scope 外（ast.rs・typeck・examples 等）を変更していない（`git status`）

## STOP conditions

改善を試みず停止して報告する:

- 「Current state」の抜粋（`ExprKind::Match` の形・`Pattern` の定義・`def_spans` のキー）が
  実コードと食い違う。
- 束縛リセットを足しても再現が緑にならない → 根本原因が想定（probe パスのムーブ合流）と
  違う。最小再現と観測を添えて報告（憶測で合流ロジックを書き換えない）。
- 修正によって既存の match／所有権テストが赤になる（本物の use-after-move を見逃す回帰が
  出た）→ 報告。束縛クリアの範囲が広すぎる可能性。

## Maintenance notes

- 将来 `Or` パターン内のペイロード束縛（`Variant(x) | Variant2(x)`）が実装されたら、
  `pattern_binding_spans` の `Or` 再帰がそのまま効く（今は Or 内束縛は未サポート）。
- `let` 文の束縛も同じ「ループ先行パスでムーブ→本パスで偽エラー」の影響を受けうる。
  showcase では `let o = Some(...)` が `match` に消費されて再代入されるため表面化しなかったが、
  ループ内 `let x = <move済みになりうる式>` で似た偽陽性が出たら、同じ「束縛時に moved を
  クリア」を `let` ハンドラにも適用する（別バグ・別プラン）。
- このバグは plan 002（showcase）が炙り出した。修正後は 002 を再実行して RPN 電卓を
  通し、`tests/codegen.rs::runs_showcase_program` を入れるのが自然な次手。
