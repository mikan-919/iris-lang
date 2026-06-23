# ADR-0011: libc 脱却に向けた汎用 syscall 原語（コンパイラは命令だけ知り、番号は std が名付ける）

## Status
Accepted・未実装（計画）。`is_null`（組み込み述語）・`RawPtr`（不透明 FFI ポインタ）に続く
「最小原語はコンパイラ、命名は std」路線の延長。実装順は本 ADR の Decision 末尾を参照。

## Context
現状の I/O・プロセス・メモリ確保は libc を `extern fn` で借りている（`fopen`/`fputs`/
`fgets`/`fclose`・`exit`・`getenv`・`malloc`/`free`・`putchar`/`puts`）。最終的には
**生成物から libc 依存を外し self-contained に近づけたい**（codegen から clang を外す長期目標
とは別レイヤーの中間目標）。

この環境は `llvm-config` 不在のため codegen は **LLVM IR テキストを出力し clang に渡す**方式。
このテキスト出力方式は、LLVM の **inline asm** をそのまま書けるため、特定の `extern` を
「`declare`＋`call`」ではなく **`syscall` 命令へ直接展開**するのに都合がよい。

論点は「どの粒度をコンパイラに知らせるか」。syscall ごとに名前付き extern（`sys_write` 等）を
codegen が個別に展開する案（B）と、汎用の syscall 原語を1つだけ知らせ、番号付けとラッパは
std 側に閉じる案（A）。iris は既に `RawPtr`/`is_null` で「コンパイラは最小原語だけ、具体的な
意味付けは std」という層構造を採っている。

## Decision
**汎用 syscall 原語をコンパイラ組み込みとして1つ用意し、Linux syscall 番号と各ラッパ
（`read`/`write`/`open`/`close`/`exit` …）は `std/os.iris` 側で iris として書く（A 案）。**

- 原語の形は引数個数別の関数群（例: `syscall0`〜`syscall6`）。codegen は名前を特別扱いし、
  x86-64 Linux の規約（番号 `rax`、引数 `rdi,rsi,rdx,r10,r8,r9`、`syscall` 命令、戻り `rax`、
  clobber `rcx,r11,memory`）に従う **inline asm** を吐く。`declare` は出さない。

  ```llvm
  ; write(fd, buf, len) 相当（番号 1）
  %r = call i64 asm sideeffect "syscall",
          "={rax},{rax},{rdi},{rsi},{rdx},~{rcx},~{r11},~{memory}"
          (i64 1, i64 %fd, i64 %buf, i64 %len)
  ```

- std はこの上に薄いラッパを書く。番号・ABI 知識はすべて std に閉じる:

  ```iris
  pub fn write(fd: i32, buf: string, len: i64): i64 { syscall3(1, fd as i64, buf as i64, len) }
  pub fn exit(code: i32): void { syscall1(60, code as i64) }
  ```

- 前提として **ポインタ→整数変換（`ptr as i64`、対象は `RawPtr`/`string`）** を `as` に追加する
  （現状の `as` は数値↔数値・`bool`→数値のみ。ADR は別途 STATUS の `as` 項に追記）。

**実装順（縦切り）**: ① `syscall` 原語 ＋ `ptr as i64` → ② `exit` か `write` を syscall 版にして
「libc 無しで 1 本動く」ことを実証 → ③ 残りの os ラッパを移行。

## Considered Options
- **B: syscall ごとに名前付き extern を codegen が個別展開**。番号・名前がコンパイラに焼き込まれ、
  syscall を増やすたびに codegen を触る。`RawPtr`/`is_null` の層構造に反する。却下。
- **A: 汎用 syscall 原語1つ＋std で命名（採用）**。コンパイラは「syscall 命令の出し方」だけ知る。
  番号・ラッパ・将来のアーキ分岐は std に閉じ、拡張で codegen を触らない。
- **libc のまま据え置き**。最も単純だが self-contained 目標に進まない。中間目標として却下。

## Consequences
- **外れるのは libc 依存であって clang ではない**。アセンブル/リンクには引き続き clang が要る
  （codegen から clang を外す長期目標は別 ADR/段で扱う）。
- **旨味が出るのは `-nostdlib` まで進めたとき**。I/O だけ syscall 化しても `malloc` が libc の
  ままなら libc をリンクし続ける。完全脱却には自前 `_start`・自前アロケータ（mmap ベース、
  [ADR-0012](0012-global-allocator-with-override.md)）・自前の文字列整形が要り、スコープが広がる。
- **アーキ/OS 固定**。inline asm の syscall は x86-64 Linux 専用（番号も Linux 固有）。現開発環境
  （WSL2 / x86-64 Linux）では問題ないが、移植時は原語の codegen にアーキ分岐が必要。
- inline asm の clobber/制約を誤ると静かに壊れるため、原語の codegen は1箇所に集約し、`write`/
  `exit` の実走（clang 実行）で回帰を張る。
