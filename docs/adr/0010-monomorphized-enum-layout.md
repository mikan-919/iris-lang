# ADR-0010: ジェネリック enum の単相化レイアウト（スカラは i64 共通・集約は per-instantiation）

## Status
Accepted・実装済み（`Option<T>`/`Result<T,E>` ＋ ユーザ定義ジェネリック enum を、スカラ／
参照／`string` ペイロード＝i64 共通レイアウト、struct 等の集約ペイロード＝per-instantiation
レイアウトで縦断対応。`tests/codegen.rs`）。

## Context
ジェネリック enum（`type Option<T> = enum { None, Some(T) }` 等）を LLVM へ落とすとき、
インスタンス（`Option<i32>`・`Option<Point>` …）ごとにペイロード記憶域をどう確保するかが問題。

既存の非ジェネリック enum は全て `%Name = type { i8, i64 }`（i8 タグ＋ i64 にキャストした
ペイロード）で表現していた。i64 スロットは数値・`bool`・参照・`string`（＝opaque ポインタ）
には十分だが、**struct 等の集約型はビット幅を超えて収まらない**（`i64` への bitcast/ptrtoint が
できない）。一方で全インスタンスを常に per-instantiation の named type にすると、スカラ系
（`Option<i32>` と `Option<bool>` …）まで無駄に型が増える。LLVM のテキスト IR には union 型が
無く、target data も使えない（`llvm-config` 不在、ADR 環境制約）ため、集約の記憶域サイズ・整列を
自前で厳密計算するのは避けたい。

## Decision
ペイロードが i64 に収まるかでレイアウトを二分する（ハイブリッド）。

- **スカラペイロード**（数値・`bool`・`char`・参照 `&T`・`string`＝ptr、未確定 `Infer`/リテラル
  も保守的にここ）: 全インスタンス共通の `%Enum = type { i8, i64 }`。型引数はレイアウトに
  影響しないため**マングルした型を出さない**。構築はペイロードを i64 へキャストして格納、
  `match` は i64 から読み戻してキャスト（既存の `cast_to_i64`/`cast_from_i64`）。
- **集約ペイロード**（struct・タプル・配列・ネストした enum 等、i64 に収まらない）:
  per-instantiation の `%Enum.Args = type { i8, <記憶域> }` を発行する。**記憶域型は、その
  インスタンスの全バリアントのペイロードのうち最大サイズの型**を採る（単一ペイロード型なら
  その型そのもの。`Option<Point>` → `{ i8, %Point }`）。整列とサイズ確定は LLVM に委ねる
  （named type の field padding を利用）。opaque ポインタのため、記憶域型と異なるバリアント
  ペイロードも GEP した `ptr` への **typed store/load** で読み書きでき、bitcast は要らない。

使用インスタンスは式の型（`TypeInfo::expr_types`）と非ジェネリック関数/メソッドの
シグネチャから集めて型宣言を出す。型レイアウトの真実源は codegen の `StructReg.enum_layouts`
（タグ＝バリアントの宣言順 index。ビルトイン Option/Result もここに登録し、typeck の
`prelude_variant` とタグ順を一致させる）。

## Considered Options
- **常に i64 共通 ＋ 大きいペイロードはヒープ box**（`{ i8, i64 }` の i64 にポインタを格納）:
  union 不要で一様だが、**アロケータが無い**（`malloc` 未配線、`alloca` は関数を跨ぐと dangling）。
  値で返す `Option<Point>` が壊れる。却下。
- **常に per-instantiation の `{ i8, [N x i8] }` バイト列 union**: 完全な union だが、N（最大
  サイズ）と整列を**自前計算**する必要があり、整列ミスでバグりやすい。却下。
- **スカラ＝i64 共通／集約＝per-instantiation（採用）**: スカラ系の型増加を避けつつ、集約は
  ペイロード型そのものを記憶域にして LLVM に整列を任せる。実装が素直で、`Option`/`Result`/
  ユーザ generic enum を同じ仕組みで通せる。

## Consequences
- スカラ generic enum（`Option<i32>` 等）は単相化してもマングル型が増えず IR が小さい。
  これにより `!`（エラー伝播）や一般イテレータ `Iterator::next() -> Option<T>` の前提が埋まる。
- 集約の記憶域は「最大サイズのペイロード型」を採るため、**複数異種ペイロードで整列要求が
  最大型 ≠ 最大整列のとき**に過小整列となりうる（保守的には最大型は十分大きいことが多いが、
  厳密ではない）。現状の対象（単一ペイロードの `Option`、同種/単純な struct）では問題に
  ならないが、限界として記録する。
- **使用インスタンス収集の限界**: 収集は式の型と非ジェネリックなシグネチャに限る。
  **ジェネリック関数の単相化の内側でのみ現れる集約 enum インスタンス**は現状未収集
  （型宣言が出ず未定義型になりうる）。必要になれば関数単相化ワークリストと統合する。
- enum レイアウトの真実源を `StructReg` に一本化した（旧 `EnumReg` は廃止）。タグの二重定義を
  避け、`llvm_ty`／構築／`match` が同じ表を引く。
