# Repository Map
_エージェント向けルーティングインデックス。作業前に該当ドキュメントを読む。_

## 概要・設計方針
- `docs/CONCEPT.md` — 言語コンセプト（不変の設計指針）; 仕様・ADR を読む前に確認。
- `docs/CONTEXT.md` — 用語集（trait/let/mut/所有権 DAG 等）; 用語が曖昧なとき参照。

## 実装状況
- `docs/STATUS.md` — パイプライン各段の対応済み/未対応一覧; 機能追加・バグ修正の前に必ず確認。

## 言語仕様
- `docs/spec/type.md` — 型システム（プリミティブ・struct・enum・参照）。
- `docs/spec/trait.md` — トレイト定義・実装・ディスパッチ。
- `docs/spec/ownership.md` — 所有権・ムーブ・借用モデル。
- `docs/spec/control.md` — 制御フロー（if/match/for/while）。
- `docs/spec/error.md` — エラーハンドリング（Result・`!` 伝播）。
- `docs/spec/function.md` — 関数定義・クロージャ。
- `docs/spec/module.md` — モジュール・`use`・可視性。
- `docs/spec/include.md` — インポート構文。
- `docs/spec/compiler.md` — コンパイラバックエンド仕様。
- `docs/spec/concurrency.md` — 並行性（未設計）。

## 設計判断（ADR）
- `docs/adr/` — ADR-0001〜0014; 既存の設計選択を覆す前に確認。主要判断:
  - 0001: `+` が型合成・`&` は参照専用
  - 0002: `!` がエラー伝播・`?` は三項演算子
  - 0003: ライフタイム注釈なし（コンパイラ自動推論）
  - 0005: ユーザ定義型は move-only 既定
  - 0010: ジェネリック enum の単相化レイアウト
  - 0011: libc 脱却の汎用 syscall 原語（命令はコンパイラ・番号は std）※計画
  - 0012: グローバル既定アロケータ＋実行時フィールドでオーバーライド（型引数にしない）※計画
  - 0014: 当面 x86-64 Linux のみを正式ターゲットとする（移植抽象は第 2 ターゲットが要るまで保留）

## 参考資料
- `docs/ai_ref/001.md` — 設計初期の議論サマリ（背景理解用）。
