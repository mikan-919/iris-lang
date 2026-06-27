# Plan 006: 移植性（x86-64 Linux 固定）の継ぎ目を文書化し、第 2 ターゲットが要るまで明示的に保留する

> **Executor instructions**: これは**保留判断を記録する軽量プラン**である。実装はしない。
> 成果物は ADR 1 本（保留の理由と将来の継ぎ目の所在）。完了後 `plans/README.md` を更新。
>
> **Drift check (run first)**: `git diff --stat 9ffa348..HEAD -- src/codegen.rs std/os.iris std/alloc.iris`
> 大きく変わっていたら下の「Current state」の所在を実コードで確認してから書く。

## Status

- **Priority**: P3（**現時点では着手非推奨＝保留**）
- **Effort**: L（実装する場合。本プラン自体は S：文書のみ）
- **Risk**: LOW（文書のみ）
- **Depends on**: none
- **Category**: direction（保留判断）
- **Planned at**: commit `9ffa348`, 2026-06-25

## Why this matters

iris の codegen は **x86-64 Linux 固定**: syscall は x86-64 の inline asm に直接展開され
（STATUS.md・`@__iris_alloc` の inline asm／`syscall0`〜`6`）、syscall 番号は Linux 値が
`std/os.iris`・`std/alloc.iris` にハードコードされている（mmap=9, munmap=11, write=1,
exit=60 等）。WASM・ARM・他 OS への移植や、将来の並行処理（`concurrency.md` 未設計）は
この継ぎ目に当たる。**しかし第 2 ターゲットが存在しない今、移植抽象を入れるのは
1 実装しかない抽象＝YAGNI**（CONCEPT.md は「自分自身のために作る」個人言語）。本プランは
「やらない」を**明示的な判断として記録**し、いざ第 2 ターゲットが要るときに何をどこで
触るかの地図を残すことが目的。これにより「移植性が無い」のが見落としか意図かが後から分かる。

## Current state（将来の継ぎ目の所在・移植時に触る場所）

- **syscall 命令の展開**: `src/codegen.rs` が `syscall0`〜`syscall6` を x86-64 Linux 規約の
  inline asm（`call i64 asm sideeffect "syscall", "={rax},{rax},{rdi},..."`）へ展開
  （STATUS.md「汎用 syscall 原語」）。アーキ変更時の第一の継ぎ目。
- **アロケータの inline asm**: `src/codegen.rs:600,613` の `@__iris_alloc`/`@__iris_free`
  （mmap/munmap inline asm・ページアライン）。
- **自前 `_start`**: codegen 生成（ADR-0012 ⑤・`-nostartfiles`）。OS/ABI 依存。
- **syscall 番号**: `std/os.iris`（open/close/read/write/exit/getenv）・`std/alloc.iris`
  （`MmapAlloc`: mmap=9, munmap=11）に Linux x86-64 値で直書き。
- 設計値（CONCEPT.md）: 個人用・明示性（暗黙ランタイムコスト禁止）。移植抽象もゼロコストで
  あるべき（実行時ディスパッチを入れない＝コンパイル時ターゲット選択）。

## Scope

**In scope**:
- `docs/adr/0014-target-x86-64-linux-only-for-now.md`（新規・保留判断と将来の継ぎ目の地図）

**Out of scope**（第 2 ターゲットが現れるまで一切やらない）:
- `src/codegen.rs` の syscall/inline-asm 抽象化 — 1 実装しかない抽象は入れない。
- WASM/ARM/他 OS バックエンド — 着手しない。
- syscall 番号の表化・ターゲット別 cfg — 不要。

## Git workflow

- Branch: `advisor/005-portability-adr`
- コミット 1 つ（ADR 追加）。メッセージ例 `docs(adr): ADR-0014 当面 x86-64 Linux 固定の保留判断`。
- push / PR は指示が無ければしない。

## Steps

### Step 1: 保留判断の ADR を書く

`docs/adr/0014-target-x86-64-linux-only-for-now.md` を、既存 ADR（`docs/adr/0012-*.md` の形）に
倣って書く。内容:

- **Status**: Accepted（保留＝当面 x86-64 Linux のみを正式ターゲットとする）。
- **Context**: 上記「Current state」の継ぎ目を列挙（codegen の inline asm・`_start`・syscall
  番号の所在を `file:line` で）。
- **Decision**: 第 2 ターゲットの具体的需要が出るまで移植抽象を**入れない**。理由は YAGNI
  （1 実装の抽象）と個人用スコープ（CONCEPT.md）。
- **Consequences**: 将来 WASM/ARM/他 OS が要るとき、(a) syscall 展開の抽象化、(b) syscall
  番号のターゲット別解決、(c) `_start`/ABI のターゲット別生成、をコンパイル時ターゲット選択
  （実行時ディスパッチ無し＝ゼロコスト）で入れる方針だけ予約しておく。

**Verify**: `docs/adr/0014-target-x86-64-linux-only-for-now.md` が存在し、3 つの継ぎ目を
`file:line` 付きで列挙している。`cat docs/adr/0012-global-allocator-with-override.md` と比べ
体裁が揃っている。

### Step 2: MAP / STATUS から ADR を参照できるようにする（任意・軽微）

`MAP.md` の ADR 一覧と `docs/STATUS.md` の「バックエンド・解析（未着手）」節から、新 ADR を
1 行で参照する（WASM ターゲットの記述の隣）。

**Verify**: `grep -rn "0014" MAP.md docs/STATUS.md` が参照を返す。

## Done criteria

ALL must hold:

- [ ] `docs/adr/0014-*.md` が保留判断＋継ぎ目の地図（`file:line`）を含む
- [ ] `src/` を一切変更していない（`git diff --stat -- src/` が空）
- [ ] `MAP.md`/`STATUS.md` から新 ADR を参照（任意・行ったなら grep で確認）
- [ ] `plans/README.md` の本計画の行を更新

## STOP conditions

停止して報告する:

- 「Current state」の継ぎ目の所在が実コードと大きく食い違う（codegen が既にターゲット抽象を
  持っている等）→ 状況を報告（保留 ADR の前提が崩れる）。
- ユーザが「いや第 2 ターゲット（WASM 等）を今やりたい」と示した → 本プランは破棄し、対象
  ターゲットを決めた上で実装設計プランを別途切る（保留ではなく着手の判断）。

## Maintenance notes

- これは**意図的な非実装の記録**。後から「移植性が無いのは見落とし?」と問われたら ADR-0014
  を指す。
- 第 2 ターゲットの需要が出たら、本 ADR の Consequences を起点に実装設計プランへ昇格する。
  そのときは「コンパイル時ターゲット選択・実行時ディスパッチ無し」（明示性・ゼロコスト）を
  崩さないこと。
