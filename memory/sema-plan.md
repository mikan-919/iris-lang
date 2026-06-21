---
name: sema-plan
description: iris-lang 意味解析を縦切りで積み上げる順序と現在地
metadata:
  type: project
---

iris-lang の意味解析は AskUserQuestion で「名前解決から」を選択。縦切りで 1 パスずつ積み上げる方針。

- 順序: **名前解決 → 型検査 → 所有権DAG**（所有権は仕様 Open Question 領域で最難のため最後）。
- 完了: 名前解決（`src/sema/resolve.rs`、`tests/resolve.rs`）。スコープ構築・未定義名/重複定義検出・複数エラーを miette でまとめ表示。`Ok/Err/Some/None` はプレリュード登録。
- 完了: 型検査（`src/sema/ty.rs` の内部型 `Ty` ＋ `src/sema/typeck.rs`、`tests/typeck.rs`）。プリミティブ＋ジェネリクス、リテラル未確定型（注釈なし let は i32/f64 へ確定）、代入/引数/戻り値の型不一致、演算子の被演算子型、呼び出しの引数個数/型、`!` の Result/Option 制約、可変性検査（不変束縛への再代入禁止）。
- 完了: 型定義（`tests/typedef.rs`）。`type Name<T> = (別名|struct|enum)` のパース、構造体リテラル `Name{...}`（if/三項の条件位置では `no_struct` フラグで抑制）、型名検証、別名は名前的型付け（リテラル代入は元の型で判定）、struct のメンバ型付け・リテラルのフィールド検査。enum は定義のみ（値の構築/分解は未対応）。ジェネリック本体は単一化未実装で寛容(Infer)。
- 完了: 所有権 DAG（`src/sema/ownership/` = typegraph.rs / flow.rs / borrows.rs / mod.rs、`tests/ownership.rs` `ownership_dag.rs` `borrow_conflict.rs`）。三層構成:
  1. 型レベルの所有権グラフ＋循環検出（無限サイズ型）。`&`/Box/Vec/Map/Set は所有辺なし、Named/Option/Result/タプル/配列/別名はインライン所有辺。
  2. 値レベルのムーブ＋借用グラフ。use-after-move、Copy判定（数値/bool/char/`&T`/全Copyタプル・別名は元型）、`&`/`&mut`・メソッド受け手は借用、再代入で再初期化、if/三項は保守的合流。ライフタイムは provenance（参照の出所）を借用グラフで追跡し到達可能性で判定 → ダングリング返却と「指す先ムーブ後の参照使用」を検出。参照引数経由は呼び出し側所有で安全。
  3. 借用競合（エイリアス規則）。同一の場所で `&mut` 排他・`&` 複数可。生存期間はスコープベース（保守的・健全だが多めに報告）。二重`&mut`・`&mut`と`&`の同時・同一呼び出し内競合を検出。再代入で借用解放。
- 完了: **トレイトシステム**（ADR-0004〜0009、`tests/traits.rs`）。typeck に `traits`/`trait_impls`/`generics`/`self_ty` を追加。`check_conformance`（メソッド提供・シグネチャ一致・スーパートレイト要求・重複実装拒否）、`collect_providers`+`resolve_trait_method`（具象型＝固有＋実装トレイト直接メソッド、ジェネリック型パラメータ＝境界をスーパートレイト辿り）でメソッド解決＋`#`修飾子＋曖昧性。ジェネリック関数 `fn f<T: Bound>` は `unify` で型引数推論＋境界充足検査。`Self`・トレイト型引数は `subst` で置換。typeck が単相化情報（`mono`: callee span→型引数）と解決済み提供元（`method_provider`: span→ラベル）を `TypeInfo` に記録しコード生成へ渡す。所有権は `==`/`!=` を読み（use_place）扱いにしてムーブしない。
- lib.rs: lex→parse→resolve→typeck→ownership。テスト総数 162（traits 17 本含む）。
- 未対応（所有権の次段）: NLL風の精密なライフタイム領域推論（最後の使用で借用終了）、参照を返却以外の長寿命の場所へ格納する一般ケース、部分ムーブ、ループ内ムーブ/借用。
- 次の候補: enum値＋match（`!`・`for`・イテレータの前提）、ジェネリック**型**の単相化、`Eq`/順序/算術トレイト、`Type.func()`（self なし関連関数）、モジュール `use`。

**Why:** 仕様の「所有権DAG」を名前通りグラフで実装。ライフタイムもグラフ到達可能性で処理（ユーザー要望）。借用競合も追加済み。
**How to apply:** NLL風領域推論が次の難所。意味解析は一通り揃ったのでコード生成も選択肢。関連する構文未確定点は [[parser-slice-assumptions]]。
