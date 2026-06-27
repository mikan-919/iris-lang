# Plan 004: 文字列補間 `` `...{expr}...` `` を `.concat` 連鎖へ脱糖して実装する（文字列値のみ・v1）

> **Executor instructions**: 上から順に。各ステップの検証コマンドを実行し期待結果を
> 確認してから次へ。「STOP conditions」に該当したら停止して報告。完了後
> `plans/README.md` を更新。
>
> **Drift check (run first)**: `git diff --stat 9ffa348..HEAD -- src/lexer.rs src/token.rs src/parser/ src/ast.rs std/prelude.iris`
> in-scope が変わっていたら下の抜粋と実コードを突き合わせ、ずれていれば STOP。

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: direction（書き心地・spec 準拠）
- **Planned at**: commit `9ffa348`, 2026-06-25

## Why this matters

文字列補間は spec（`docs/spec/type.md`）が**明示的に規定**している唯一の文字列機能なのに
未実装（STATUS.md「文字列補間（バッククォート `` `...{expr}...` ``）」）。`+` は型合成
（ADR-0001）で連結に使えないため、今は文字列の組み立てが `a.concat(b).concat(c)` の手書き
連鎖になり、CONCEPT.md の「書き心地」に反する。補間を**既存の `.concat` への脱糖**として
実装すれば、新しい codegen 経路を増やさずに spec を満たせる。

**重要な制約（先に読むこと）**: 現状 iris には汎用の値→文字列変換（`to_string`/`Display`）
が無い。文字列化できるのは `string` 値だけ（`print_int` は stdout に書くだけで文字列を
返さない）。よって **v1 は補間対象を `string` 型に限定**する。整数等の補間
（`` `count: {n}` `` で `n: i32`）は `to_string` 設計が要るため**本プランの対象外**とし、
型検査で明確なエラーにして将来へ送る。これは妥当な縦切り（書き心地の素を入れつつ、別設計
が要る部分は線引き）。

## Current state

- 文字列リテラルの字句解析:
  ```rust
  // src/lexer.rs:135
  fn lex_string(input: LSpan) -> nom::IResult<LSpan, Token> {
      let start = input;
      let (mut input, _) = char('"')(input)?;   // ダブルクォートのみ。バッククォート無し
      // ... エスケープ処理 ... TokenKind::Str(value) を返す
  }
  // src/lexer.rs:96 のディスパッチに lex_string が登録されている
  ```
- トークン: `src/token.rs:10 Str(String)`。AST 式: `src/ast.rs:242 Str(String)`（式側）。
- 連結は `impl string { fn concat(&self, other: string): string }`（`std/prelude.iris:41`、
  自動前置）。`a.concat(b)` は固有メソッド静的ディスパッチで解決済み（`tests/codegen.rs:683
  runs_string_concat` が実走検証）。
- バッククォート（`` ` ``）は現状どのトークンにも使われていない（`grep` 済み）。
- 設計値（CONCEPT.md）: 改行が文区切り・コメントは日本語。

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| ビルド | `cargo build` | exit 0 |
| 字句/構文テスト | `cargo test --test parse` | 緑 |
| 型検査テスト | `cargo test --test typeck` | 緑 |
| codegen 実走 | `cargo test --test codegen interp` | 追加テスト pass |
| 手動確認 | `cargo run -- run /tmp/interp.iris` | exit 0・期待出力 |
| 全テスト | `cargo test` | 緑 |

## Scope

**In scope**:
- `src/lexer.rs` — バッククォート文字列の字句解析（補間部分を分割）
- `src/token.rs` — 必要なら補間トークン種を追加
- `src/parser/mod.rs`・`src/ast.rs` — 補間を `.concat` 連鎖の AST へ脱糖
- `src/sema/typeck.rs` — 補間対象が `string` 型かの検査・非 string に明確なエラー
- `tests/parse.rs`・`tests/typeck.rs`・`tests/codegen.rs` — 回帰テスト追加
- `docs/spec/type.md`・`docs/STATUS.md` — 実装範囲を反映

**Out of scope**:
- 非 string 値の補間（`{n}` で `n: i32` 等）— `to_string`/`Display` 設計が要る。型検査で
  エラーにし**本プランでは実装しない**（将来の別プラン）。
- `std/prelude.iris` の `concat` 実装 — 既存をそのまま脱糖先に使う（変更しない）。
- 文字列のスライス・`char` 単位索引 — 無関係。触らない。

## Git workflow

- Branch: `advisor/003-string-interpolation`
- 縦切りの段ごとにコミット（lexer→parser→typeck→tests）。メッセージは repo 準拠
  （例 `feat(string): 文字列補間を .concat 脱糖で実装`）。
- push / PR は指示が無ければしない。

## Steps

### Step 1: バッククォート文字列を字句解析する

`src/lexer.rs` に `lex_interp_string`（仮称）を足し、`src/lexer.rs:96` のディスパッチへ
`lex_string` と並べて登録する。`` ` `` で開始し `` ` `` で終了。中身を**リテラル断片**と
**`{ expr }` 補間部**の列に分割する。`lex_string`（行 135–183）のエスケープ処理を踏襲する
（`\n` 等）。`{` は補間開始、`}` で閉じる。`\{` で literal の `{` をエスケープ可能にする。

実装方針は 2 択（どちらでも可、後段が成立する方を選ぶ）:
- (a) lexer 段で `` `...` `` 全体を 1 トークン `TokenKind::InterpStr(Vec<InterpPart>)` に
  畳む（`InterpPart = Lit(String) | Expr(Vec<Token>)`）。
- (b) lexer 段で `InterpStart`/`InterpLit`/`InterpExprStart`/`InterpEnd` 等の境界トークンを
  出し、parser 段で組む。

補間部の式は**既存の式字句解析を再利用**できる形にする（`{` と `}` の対応を数えて括る）。

**Verify**: `cargo test --test parse` が緑。`` `ab` ``（補間なし）と `` `a{x}b` `` の両方が
トークン化されることを確認する小テストを `tests/parse.rs` に 1 件追加し pass。

### Step 2: 補間を `.concat` 連鎖の AST へ脱糖する

`src/parser/mod.rs` で、補間文字列を**文字列式の `.concat` 連鎖**に脱糖する。
`` `a{x}b{y}` `` → `"a".concat(x).concat("b").concat(y).concat("")`（末尾の空連結は省略可）。
補間部 `{expr}` の `expr` は既存の式パーサで解析した AST ノードをそのまま挿入する。
**AST に新ノードを足さず**、既存の `Member`+`Call`（`.concat(...)`）と `Str` リテラルで
構成する（codegen を一切変更しないため）。空断片は畳む（`` `{x}` `` → `x`、ただし `x` が
string であること。Step 3 で検査）。

**Verify**: `cargo run -- --emit-llvm /tmp/i.iris`（`/tmp/i.iris` に
`fn main(): i32 { let a = "x"\n let s = ` + "`h{a}!`" + `\n return s.len() }`）が IR を出し、
`call` が `@string.concat` を含む。

### Step 3: 型検査で補間対象が `string` であることを要求する

脱糖後は `.concat(expr)` になるので、`expr` が `string` でなければ既存の `concat` 引数型
検査が拾う。ただしエラーメッセージが分かりにくいなら、`src/sema/typeck.rs` で補間部に
特化した診断（「補間 `{...}` の中身は `string` 型である必要があります（数値等の補間は
未対応）」）を miette で出す。span は補間式を指す。

**Verify**: `cargo test --test typeck` が緑。`` `n={n}` ``（`n: i32`）が**明確なエラー**に
なる小テストを `tests/typeck.rs` に追加し pass（パニックや不明瞭エラーでないこと）。

### Step 4: 実走テストと spec/STATUS 更新

`tests/codegen.rs` に、`runs_string_concat`（`tests/codegen.rs:683`）と同型の実走テストを
追加: `let name = "world"` → `` let s = `hello {name}!` `` → `return s.len()`（"hello world!"
= 12）。`run_exit_code` で `Some(12)` を検証（`if let Some` 包み・clang 不在はスキップ）。
`docs/spec/type.md` と `docs/STATUS.md` の補間項を「実装済み（v1・string 値のみ）」に更新。

**Verify**: `cargo test --test codegen interp` が緑。`cargo test` 全体が緑。

## Test plan

- `tests/parse.rs`: 補間なし `` `ab` `` ／補間あり `` `a{x}b` `` のトークン化（happy + 境界）。
- `tests/typeck.rs`: `string` 補間 OK ／非 string 補間が明確エラー（線引きの回帰）。
- `tests/codegen.rs`: `` `hello {name}!` `` の実走で長さ 12（手本: `runs_string_concat`
  `tests/codegen.rs:683`）。
- 検証: `cargo test` → 既存緑のまま＋新規 pass。

## Done criteria

ALL must hold:

- [ ] `` `a{x}b` `` がトークン化・脱糖され、`x: string` でコンパイル・実走できる
- [ ] `cargo test --test parse` / `--test typeck` / `--test codegen` がいずれも緑
- [ ] 非 string 補間が**パニックせず**明確な miette エラーになる（テストで固定）
- [ ] `src/codegen.rs` を変更していない（脱糖のみ。`git diff --stat` で確認）
- [ ] `docs/spec/type.md`・`docs/STATUS.md` の補間項を更新
- [ ] in-scope 外を変更していない（`git status`）
- [ ] `plans/README.md` の本計画の行を更新

## STOP conditions

停止して報告する:

- `src/lexer.rs:135` の `lex_string` や `src/lexer.rs:96` のディスパッチ構造が抜粋と違う。
- 補間部の式字句解析で `{` `}` のネスト（補間内に struct リテラルや別の補間）が必要になり
  設計が膨らむ → v1 は**ネスト非対応**で線引きしてよいか確認する（最初の `}` で閉じる
  単純規則で実装し、ネストは将来送りにする旨を STATUS に明記して進める。判断に迷えば報告）。
- 脱糖だけでは codegen 変更が避けられないと判明 → 報告（本プランは codegen 不変が前提）。

## Maintenance notes

- v1 は **string 値の補間のみ**。整数等の補間は `to_string`/`Display` トレイトの設計が前提
  で、これは別の設計判断（クロージャ plan 003 と同様に ADR を切ると良い）。本プランの
  STATUS 追記にこの線引きを残すこと。
- レビュアは「codegen を本当に触っていないか（脱糖で閉じているか）」「非 string 補間の
  エラーが分かりやすいか」を見る。
- 連結結果はヒープ（`concat` の `malloc`、現状リーク許容＝既存仕様）。補間が増えると一時
  文字列が増えるが、Vec/string の leak 許容方針（STATUS）と同じ扱いで v1 は許容。
