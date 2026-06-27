# Plan 050: クロージャ／ラムダの本実装（縦切り・ADR-0013 が前提）

> **骨子のみ**。本プランは [ADR-0013](../docs/adr/0013-closures.md) の設計確定を前提とする
> 実装フェーズの**粗い縦切り順**である。各段の詳細・正確な `file:line`・テスト名は、着手時に
> 当該段の現状コードを読んで詰める（この骨子のまま executor に渡さない — 設計承認後に
> 各段を個別プラン化するか、段ごとに drift check して具体化すること）。

## Status
- **Priority**: P2（ADR-0013 承認後）
- **Effort**: L（パイプライン 5 段の縦断）
- **Risk**: MED（捕獲推論が所有権 DAG の新規拡張）
- **Depends on**: ADR-0013（捕獲＝DAG 推論・型＝組込み関数型・単相化 codegen）の承認
- **Category**: 言語コア（表現力）
- **Planned at**: commit `bdc27b4`, 2026-06-27（骨子）

## 前提（ADR-0013 の確定事項）
- 構文 `(params): Ret -> expr | { ... }`。期待型がある位置で型省略可。
- 捕獲は所有権 DAG が捕獲ごとに借用／ムーブを推論（`move` 注釈なし）。
- クロージャ型 = 組込み関数型 `(T): U`。`Fn`/`FnMut`/`FnOnce` トレイトは作らない。
- codegen は捕獲環境 struct ＋ 既存 monomorphization worklist（`@f.Type`）に載せる。
- **v1 は非エスケープクロージャ（アダプタ即時消費）に限定**してよい。エスケープ捕獲
  （戻り値・格納のムーブ捕獲）は v2。

## 縦切り順（各段に検証）

### Phase 1: lexer / parser
- `(params): Ret -> body` のラムダ式を構文解析（`->` は既存 `Arrow` トークン流用、STATUS.md）。
  式本体・ブロック本体の両方。期待型のない位置での型省略も AST 上は許す（型は typeck）。
- 関数型 `(T): U` を型注釈位置でパース（既に spec にあるが実装確認）。
- **検証**: `cargo run -- examples/<lambda>.iris` が AST を出す。`cargo test --test parser` 緑＋
  ラムダの parse テスト追加。

### Phase 2: typeck
- ラムダに組込み関数型 `(T): U` を付与。期待型からの引数型推論（双方向）。
- アダプタ引数 `f: (T): U` への適合検査。クロージャリテラルごとに無名具象型を割り当て。
- **検証**: `cargo test --test typeck` 緑＋ラムダの型付け／型不一致テスト追加。

### Phase 3: ownership（最重要・新規拡張）
- **flow.rs**: 捕獲を借用／ムーブイベントとして記録。捕獲した借用の寿命＝クロージャ寿命の
  provenance 伝播。ダングリング検出（捕獲参照がクロージャより先に死ぬ）を既存機構で。
  v1 は非エスケープ前提なら借用／値コピー捕獲のみで足りる。
- **borrows.rs**: `&mut` 捕獲のスコープベース排他。
- **検証**: `cargo test --test ownership` 緑＋（a) 正当な借用捕獲が通る（b) ダングリング捕獲が
  赤になる、のテスト追加。

### Phase 4: codegen
- 各クロージャを捕獲環境 struct ＋ 環境を先頭引数に取る関数へ。アダプタを具象クロージャ型
  ごとに既存 monomorphization worklist（`codegen.rs:683,751`・`mono_symbol`）で単相化。
  vtable／関数ポインタは使わない。
- **検証**: `cargo test --test codegen` 緑。`run_exit_code` 流儀でクロージャを呼ぶ実走テスト
  （clang リンク・exit code 検証）追加。

### Phase 5: std アダプタ ＋ 受け入れ
- `std/prelude.iris` に `Iterator<T>` の `map`/`filter`/`fold` を追加（ADR-0007 機構＋クロージャ）。
- **検証**: `docs/spec/function.md` のアダプタ例（`v.iter().map(...)` 等）が実走する統合テストを
  `tests/codegen.rs` に追加し緑。これが本プランの最終受け入れ基準。

## Done criteria（本実装）
- [ ] ラムダ式が lexer→codegen まで縦断し、実走テスト（clang）で正しい exit code を出す
- [ ] 捕獲が DAG で借用／ムーブ推論され、ダングリング捕獲が診断される
- [ ] `map`/`filter`/`fold` が std にあり、spec のアダプタ例が実走する
- [ ] 全 `cargo test` 緑
- [ ] STATUS.md のラムダ／高階関数の対応状況を更新

## STOP conditions
- 捕獲のエスケープ解析が v1 スコープに収まらない → 非エスケープ限定で切り、エスケープ捕獲を
  別プランへ送る判断をユーザに確認。
- codegen で単相化に載らない事態（ADR-0013 のスパイク結論と食い違う）→ 停止して報告。
- ADR-0013 と実装が食い違いそうなら、勝手に設計を変えず STOP して ADR 改訂を相談。
