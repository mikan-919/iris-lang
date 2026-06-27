# Plan 003: クロージャ／ラムダの設計を ADR で確定する（設計スパイク・コード本実装は別）

> **Executor instructions**: これは**設計プラン**である。成果物は実装コードではなく
> **ADR（設計文書）＋最小スパイク**。`src/` の本実装には踏み込まない（スパイクは捨てる
> 前提のブランチで可）。各ステップの検証を満たしてから次へ。「STOP conditions」に該当
> したら停止して報告。完了後 `plans/README.md` を更新。
>
> **Drift check (run first)**: `git diff --stat 9ffa348..HEAD -- docs/adr/ docs/spec/function.md std/prelude.iris`
> 食い違いがあれば下の抜粋と実コードを突き合わせ、ずれていれば STOP。

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED（設計判断が後続実装を左右する。コード変更自体は小）
- **Depends on**: none（ただし plan 002 の showcase が「高階関数が無くて困る」具体例を出す）
- **Category**: direction（設計）
- **Planned at**: commit `9ffa348`, 2026-06-25

## Why this matters

iris にはクロージャ／ラムダが**全く無い**（`grep -ri "closure\|lambda" src/` が空。STATUS.md
でも `ラムダ (x): T -> expr` は「未実装」）。これが要石の欠落になっている: `Iterator<T>`
トレイトは完全実装済み（ADR-0007）なのに、**`map`/`filter`/`fold` アダプタが std に一つも
無い**（`grep "map\|filter\|fold" std/prelude.iris` が空）。クロージャが無いとアダプタが
書けないため、構築済みのイテレータ機構が生の `for` でしか使えない。クロージャはまた、
**注釈なし所有権モデル（ADR-0003）が最も試される場所**でもある: 変数をムーブで捕獲するか
借用で捕獲するかを、ライフタイム注釈なしに所有権 DAG が推論しなければならない。だから
これは「書けば終わり」ではなく**設計の分岐点**であり、いきなり実装させると破綻する。本プラン
は分岐を ADR で潰してから実装プランを切るためのスパイクである。

## Current state

- クロージャ関連コードは存在しない（`grep` 確認済み）。関数は `fn name(params): Ret { ... }`
  のみ（`docs/spec/function.md`）。
- 関連する確定済み ADR（`docs/adr/`、本設計が矛盾してはいけない）:
  - **ADR-0003 ライフタイム注釈なし** — コンパイラが所有権 DAG から推論。捕獲の寿命も
    注釈無しで決める必要がある。
  - **ADR-0005 ユーザ型は move-only 既定** — 捕獲のデフォルトもこの哲学に整合させる。
  - **ADR-0006 静的ディスパッチ・トレイトオブジェクト無し** — クロージャを `dyn Fn` 的な
    トレイトオブジェクトで表すのは ADR-0006 に反する。単相化前提で設計する必要がある。
  - **ADR-0007 ジェネリックトレイト・関連型なし** — `Fn`/`FnMut`/`FnOnce` を関連型なしで
    どう表すか（あるいはトレイト無しの組込みクロージャ型にするか）が論点。
- 所有権 DAG は三層（`src/sema/ownership/`: `typegraph.rs`/`flow.rs`/`borrows.rs`）。捕獲は
  flow.rs（ムーブ/借用）と borrows.rs（エイリアス規則）の両方に影響する。
- 既存の単相化機構: ジェネリック関数は呼び出しごとに `@f.Type` を emit（STATUS.md・
  ADR-0006）。クロージャ実装はこの単相化に載せられる可能性が高い。
- 設計値（CONCEPT.md）: 所有権を呼び出し側に露出しない・`mut`/`ref`/`nobind` は宣言側。

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| 既存 ADR の形を見る | `cat docs/adr/0007-generic-traits-no-associated-types.md` | ADR テンプレ確認 |
| スパイクのコンパイル試行 | `cargo run -- --emit-llvm /tmp/spike.iris` | IR か、未対応エラー |
| 既存テスト | `cargo test` | 緑（スパイクで壊さない） |

## Scope

**In scope**:
- `docs/adr/0013-closures.md`（新規・設計判断の確定。番号は既存最大 0012 の次）
- `docs/spec/function.md`（クロージャ構文節を追記）
- `plans/050-closures-impl.md`（任意・本実装の後続プランの骨子。番号は実装フェーズ帯）
- 捨てスパイク用の一時 `.iris`（`/tmp` 等・リポジトリにコミットしない）

**Out of scope**:
- `src/` のコンパイラ本実装 — 本プランでは**書かない**。ADR が決まってから別プランで。
- 既存 ADR（0001–0012）の変更 — 矛盾を見つけたら STOP して報告（覆すのは別判断）。

## Git workflow

- Branch: `advisor/002-closures-design`
- コミットは ADR・spec で分ける。メッセージは repo 準拠（例 `docs(adr): ADR-0013 ...`）。
- push / PR は指示が無ければしない。

## Steps

### Step 1: 設計の分岐を洗い出し、各々に推奨を付ける

`docs/adr/0013-closures.md` を起こし、最低限**次の決定**を埋める（各々に「決定」と「理由」）:

1. **構文**: STATUS.md 既出の `(x): T -> expr` を採用するか。ブロック本体
   `(x): T -> { ... }` を許すか。引数型・戻り型の省略と推論の範囲。
2. **捕獲セマンティクス（最重要）**: 捕獲のデフォルトは借用かムーブか。ADR-0005（move-only）
   と ADR-0003（注釈なし）に整合する規則は何か。Copy 型は値コピー捕獲・非 Copy は？
   `mut` 捕獲（捕獲変数の書き換え）をどう表すか（宣言側 `mut` 哲学に沿わせる）。
3. **型表現**: クロージャ型をどう表すか。トレイト（`Fn`/`FnMut`/`FnOnce` 相当）にするか、
   組込みクロージャ型にするか。ADR-0006（トレイトオブジェクト無し・静的のみ）／ADR-0007
   （関連型無し）の制約下で、**単相化で消える**設計にできるか。
4. **所有権 DAG への載せ方**: 捕獲を flow.rs のムーブ/借用イベントとして、borrows.rs の
   エイリアス規則としてどう表すか（捕獲した借用の寿命＝クロージャの寿命）。ダングリング
   検出（捕獲した参照がクロージャより先に死ぬ）の扱い。
5. **codegen 表現**: 捕獲環境を struct に詰めて関数ポインタ＋環境ペアにするか、単相化で
   各クロージャを専用関数＋環境 struct にするか（既存のジェネリック単相化 `@f.Type` に
   載るか）。

**Verify**: `docs/adr/0013-closures.md` が上記 5 論点すべてに「決定」と「理由」を持つ。

### Step 2: 最小スパイクで設計の通る/通らないを確認する

捨て前提で、設計のうち**最も不確実な 1 点**（通常は捕獲の所有権 or codegen 表現）を、
ごく小さく手で検証する。例: 捕獲を struct ＋ `@f.Type` 単相化に落とせるか、`map` 1 個分の
IR を手書きして clang で通るか。本実装はしない — 設計が物理的に成立するかの確認だけ。

**Verify**: スパイクの知見（成立する／この点が問題、の結論）を ADR の「Consequences」に
1 段落書く。スパイクコードはコミットしない。

### Step 3: イテレータアダプタの最小目標を spec に書く

`docs/spec/function.md`（または `docs/spec/trait.md`）に、クロージャが入った暁に書ける
最初の目標として `Iterator` の `map`/`filter`/`fold` の**意図する形**を 1 例ずつ示す
（実装はしない）。これが本実装プランの受け入れ基準の素になる。

**Verify**: spec にクロージャ構文節＋アダプタ目標例が載っている。

### Step 4: 本実装の後続プラン骨子を残す

`plans/050-closures-impl.md` に、ADR の決定を前提とした実装フェーズの骨子（lexer→parser→
typeck→ownership→codegen の縦切り順、各段の検証）を粗く書く。詳細はこの設計が承認されて
から詰める。

**Verify**: `plans/050-closures-impl.md` が存在し縦切り順を含む。

## Test plan

- 本プランは設計のためテストコードは書かない。`cargo test` が**緑のまま**であること
  （スパイクで既存を壊していない）だけ確認する。
- 本実装プラン（050）側で、`tests/codegen.rs` の `run_exit_code` 流儀に従ったクロージャ
  実走テストと `map`/`filter`/`fold` の実走テストを書く旨を骨子に明記する。

## Done criteria

ALL must hold:

- [ ] `docs/adr/0013-closures.md` が Step 1 の 5 論点を決定済み（各々「決定」＋「理由」）
- [ ] ADR の「Consequences」にスパイクの結論が 1 段落ある
- [ ] `docs/spec/function.md` にクロージャ構文＋アダプタ目標例が載った
- [ ] `plans/050-closures-impl.md`（実装骨子）が存在
- [ ] `cargo test` が緑（スパイクで回帰なし）
- [ ] in-scope 外（特に `src/`）を変更していない（`git status`）
- [ ] `plans/README.md` の本計画の行を更新

## STOP conditions

停止して報告する:

- 設計が既存 ADR（特に 0003 注釈なし・0006 静的のみ・0007 関連型無し）と**両立できない**
  ことが判明 → どの ADR とどう衝突するかを報告（ADR を覆すかはユーザ判断）。
- スパイクで「この設計は codegen で物理的に成立しない」と分かった → 別案へ振り直す前に報告。
- 捕獲セマンティクスがどうしても 1 案に絞れない → 候補と各々のトレードオフを並べて報告
  （ユーザに決めてもらう。勝手に実装へ進まない）。
- 「Current state」の ADR 抜粋が実ファイルと食い違う。

## Maintenance notes

- この ADR が後続の全クロージャ実装（plan 050）と `Iterator` アダプタの土台になる。
- レビュアは「捕獲のデフォルトが ADR-0005 と整合するか」「トレイトオブジェクト無し
  （ADR-0006）を崩していないか」を最重点で見る。
- 本実装は別プラン。ここで決め切らずに実装へ進むと、注釈なし所有権の捕獲推論で破綻する
  ため、ADR 承認を必ず挟む。
