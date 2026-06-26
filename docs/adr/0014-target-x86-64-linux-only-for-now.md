# ADR-0014: 当面は x86-64 Linux のみを正式ターゲットとする（移植抽象は第 2 ターゲットが要るまで入れない）

## Status
Accepted。移植性に関する**保留判断の記録**（意図的な非実装）。実装変更を伴わない方針 ADR。
[ADR-0011](0011-syscall-primitive-for-libc-independence.md)（syscall 原語）/
[ADR-0012](0012-global-allocator-with-override.md)（アロケータ）で導入した self-contained
ランタイムは、いずれも x86-64 Linux に固定されている。本 ADR はそれを「見落とし」ではなく
「現時点の意図された選択」として明記し、将来移植するときに触る継ぎ目の地図を残す。

## Context
codegen は **x86-64 Linux 固定**であり、その依存は次の継ぎ目に集中している（移植時に触る場所）:

- **syscall 命令の展開**: `syscall0`〜`syscall6` を x86-64 Linux 規約（番号を `rax`、引数を
  `rdi`/`rsi`/`rdx`/`r10`/`r8`/`r9`、`syscall` 命令）の inline asm へ展開する。
  `src/codegen.rs:2890`〜（`syscall_arity` 判定 → `call i64 asm sideeffect "syscall", ...` 生成。
  実体は `src/codegen.rs:2908` 付近）。アーキ変更時の第一の継ぎ目。
- **アロケータの inline asm**: `@__iris_alloc`（mmap・ページアライン）が `src/codegen.rs:600`、
  `@__iris_free`（munmap）が `src/codegen.rs:613`。x86-64 Linux の mmap/munmap inline asm を直書き。
- **自前 `_start`（OS エントリポイント）**: `fn main` がある場合に `@_start` を生成する
  （`src/codegen.rs:774` 以降、生成は `src/codegen.rs:792`〜`826`）。`-nostartfiles`
  （`src/codegen.rs:775`）で crt0.o を含めずリンクするため、`_start` と exit syscall（番号 60、
  `src/codegen.rs:786` 付近）を自前で提供する。OS/ABI 依存。
- **syscall 番号の直書き（Linux x86-64 値）**: `std/os.iris` に open=2（`std/os.iris:25`）・
  close=3（`std/os.iris:34`）・read=0（`std/os.iris:40`）・write=1（`std/os.iris:46`）・
  exit=60（`std/os.iris:86`）・getenv（`/proc/self/environ` スキャン、`std/os.iris:125`）。
  `std/alloc.iris` の `MmapAlloc` に mmap=9（`std/alloc.iris:38`）・munmap=11（`std/alloc.iris:45`）。

WASM・ARM・他 OS への移植や、将来の並行処理（[concurrency.md](../spec/concurrency.md) は未設計）は
これらの継ぎ目に当たる。一方、現時点で **第 2 ターゲットは存在しない**。iris は
[CONCEPT.md](../CONCEPT.md) の通り「自分自身のために作る」個人言語であり、明示性
（暗黙のランタイムコストを置かない）を核心に据える。1 実装しかない段階で移植抽象を入れることは、
**1 実装しかない抽象＝YAGNI** であり、設計を複雑にするだけで何も守らない。

## Decision
**第 2 ターゲットの具体的な需要が出るまで、移植抽象を一切入れない。当面は x86-64 Linux のみを
正式ターゲットとする。** 上記の継ぎ目（syscall 展開・inline asm アロケータ・`_start`/ABI・
syscall 番号）は x86-64 Linux 値のまま据え置く。理由:

- **YAGNI**: 抽象は最低 2 実装で初めて元が取れる。1 実装の抽象は早すぎる一般化。
- **個人用スコープ**（CONCEPT.md）: 現状の対象環境は x86-64 Linux のみで、他ターゲットの実需が無い。

## Considered Options
- **今 syscall 展開・番号をターゲット別 cfg / 表へ抽象化する**: 第 2 ターゲットが無いため
  分岐は常に同じ枝しか通らず、抽象の検証もできない。却下（YAGNI）。
- **WASM/ARM/他 OS バックエンドを先行実装する**: 実需が無く、保守対象だけが増える。却下。
- **保留を明文化し継ぎ目の地図だけ残す（採用）**: コストゼロで、後から「移植性が無いのは
  見落としか意図か」が一意に判別でき、移植時の起点も残る。

## Consequences
- 生成物は x86-64 Linux でのみ動く。他アーキ/OS では動かないが、これは**意図された制約**である
  （本 ADR が根拠）。「移植性が無いのは見落とし?」と問われたら本 ADR を指す。
- 将来 WASM/ARM/他 OS の実需が出たときは、本 ADR を起点に**実装設計プランへ昇格**し、次を入れる:
  - (a) **syscall 展開の抽象化**（`src/codegen.rs:2890` 付近のアーキ別 inline asm 生成）。
  - (b) **syscall 番号のターゲット別解決**（`std/os.iris`・`std/alloc.iris` の直書き値をターゲット別に）。
  - (c) **`_start`/ABI のターゲット別生成**（`src/codegen.rs:774`〜・`@__iris_alloc`/`@__iris_free`）。
- そのときも **コンパイル時ターゲット選択（実行時ディスパッチ無し＝ゼロコスト）** を崩さないこと
  （明示性・暗黙コスト禁止、CONCEPT.md）。移植抽象もゼロコストであるべきで、実行時の動的選択は入れない。
