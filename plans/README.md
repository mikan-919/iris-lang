# 改善計画インデックス

`/improve` による監査から生成した実装計画。各計画は**独立して**実行できるよう自己完結
している（実行は別モデル／エージェントを想定。プランを最後まで読み、STOP 条件を守り、
終わったら自分の行を更新する）。

- **001**: standard 監査（`34e8939` 時点・全パイプライン直接監査）由来。**実装済み**。
- **002–006**: `/improve next`（direction/ロードマップ監査・基準コミット `9ffa348`・
  2026-06-25）由来。「次に何を作るか」の**選択肢**であり互いに独立。全部を順にやる必要は
  ない。迷ったら推奨順に従う。

## 実行順 / 状態

| # | 計画 | カテゴリ | 見積 | リスク | 依存 | 状態 |
|---|------|---------|------|--------|------|------|
| 001 | [非 enum/bool match を catch-all 必須に（UB 修正）](001-match-exhaustiveness-catchall.md) | 正しさ/健全性 | S〜M | LOW | なし | DONE（worktree-agent-abf256419b146a053、未マージ） |
| 002 | [実用規模の iris プログラム＋統合テスト（機能の合成を実証）](002-showcase-integration-program.md) | direction/tests | S | LOW | なし | DONE（commit `e52b649`。初回 RPN 版は Vec.pop/索引代入の欠落で論理破綻〈35 でなく 3〉→ append-only 左結合電卓に全置換。`runs_showcase_program` 実走回帰で exit 35 を確認・全テスト緑） |
| 007 | [ループ内 match アーム束縛の偽 use-after-move を修正](007-ownership-loop-match-binding-move.md) | 正しさ/完全性 | S | LOW | なし | DONE/MERGED（`feat/first/nom-0` の 04a28cd にマージ済み。再現赤→緑・全テスト緑・実 double-move は引き続き検出を確認済み） |
| 008 | 空リスト literal `[]` の要素型が `_` のまま codegen に漏れる（typeck が期待型を伝播せず） | 正しさ/健全性 | S〜M | LOW | なし | DONE（`infer_struct_lit` がフィールド注釈の期待型で `finalize_array_lit` を呼ぶよう修正。`tests/codegen.rs::runs_struct_field_empty_vec` で再現赤→緑・全テスト緑） |
| 003 | [クロージャ／ラムダの設計 ADR（設計スパイク・実装は別）](003-closures-design-spike.md) | direction（設計） | L | MED | なし | DONE（`bdc27b4` 上で senior モデル＋ユーザ協働でドラフト。捕獲セマンティクス＝**所有権 DAG が捕獲ごとに借用/ムーブ推論**をユーザ決定。ADR 番号は plan が想定した `0013` が未使用の欠番だったため [ADR-0013](../docs/adr/0013-closures.md) で充当〈drift: 0014 が先に着地〉。spec＝`function.md` にクロージャ捕獲節＋map/filter/fold 目標例。実装骨子＝[plan 050](050-closures-impl.md)。docs のみ変更＝`cargo test` は構造的に不変。**本実装は plan 050 で別途・ADR 承認前提**） |
| 004 | [文字列補間 `` `...{expr}...` `` を `.concat` 脱糖で実装（string 値のみ・v1）](004-string-interpolation.md) | direction（書き心地） | M | LOW | なし | DONE（worktree-agent-a75fb9df3faf60441 / branch `advisor/003-string-interpolation`、未マージ。レキサ `InterpStr`→パーサ `"".concat(...)` 脱糖。codegen 不変を確認。parse/typeck/codegen 全緑・実走 len=12・非 string 補間は型エラー〈非パニック〉。docs 反映済み） |
| 005 | [読み取り系 libc 文字列関数を iris 化し既定経路を libc 非依存に](005-drop-libc-readonly-strfns.md) | direction（self-contained） | M | LOW | なし | DONE/MERGED（`feat/first/nom-0` の `9329f28` にマージ済み。`strlen`/`strcmp`/`strncmp` をバイトループ iris 実装へ。codegen 不変・`@strcmp` シンボル据え置き。新 `runs_without_libc_for_string_readonly_path` が `-nostdlib -Wl,--gc-sections` で実走 exit 9・全 113 codegen テスト緑・全スイート緑。2 つの正当な逸脱→下記） |
| 006 | [移植性（x86-64 Linux 固定）の継ぎ目を文書化し明示的に保留](006-portability-syscall-seam.md) | direction（保留判断） | S（文書） | LOW | なし | DONE/MERGED（`feat/first/nom-0` の `bdc27b4` にマージ済み。ADR-0014 を新規作成・継ぎ目 13 件を `file:line` 付きで地図化。MAP.md/STATUS.md から参照。`src/` 不変。seam 引用は実コード〈9329f28〉と一致を確認済み。target 抽象は不在＝保留 ADR の前提は健全） |

状態値: TODO | IN PROGRESS | DONE | BLOCKED（一行理由） | REJECTED（一行理由）

001 は副次バグ（`or` 内ワイルドカードを catch-all と認識しない数値 exhaustiveness の
誤検出）も同時に修正する。

## direction プラン（002–006）の推奨順と依存

技術的な hard 依存は無い（002–006 は独立に着手可能）。推奨実行順は **002 → 004 → 003 → 005 → 006**。

- **002 を最初に**: 最安・最高シグナル。実装済み機能が本当に合成できるかを 1 本の現実的な
  プログラムで検証し、継ぎ目のバグを炙り出す。003/004 がどこで効くかの判断材料にもなる。
- **004（補間）はその次の書き心地の easy win**（codegen 不変・脱糖のみ）。非 string 補間は
  `to_string`/`Display` 設計が前提で v1 対象外（プラン内で線引き）。
- **003（クロージャ）は最高 ceiling だが設計フォーク**: クロージャが入れば `Iterator` の
  map/filter/fold アダプタ（現状ゼロ）が書ける。注釈なし所有権（ADR-0003）下の捕獲
  セマンティクスを ADR で確定してから本実装（プラン内の 050 骨子）へ。**実装前に必ず ADR
  承認を挟む**。
- **005（libc 排除）は self-contained ランタイム（ADR-0011/0012）の完遂**だが、ユーザ可視
  価値は 002–004 より低い内部目標。**前提**: 残りの libc（strcpy/strcat/memset/malloc）排除
  には索引代入 `buf[i] = v` の codegen 追加が必要で未実装（別途プラン化が要る）。005 は前提
  なしで今刈れる読み取り系のみ。
- **006（移植性）は「やらない」判断の記録**: 第 2 ターゲット（WASM 等）の需要が無い今、
  移植抽象は YAGNI。保留を ADR に明文化するだけ。需要が出たら実装設計プランへ昇格。

## 監査範囲の注記

002–006 は `/improve next` のため **direction（features/roadmap）カテゴリのみ**を監査した。
correctness・security・perf・tech-debt の全面監査は今回していない（001 が前回 standard 監査
の成果）。バグ・負債の表が欲しければ通常の `/improve`（standard）を別途実行のこと。なお
plan 002 の実行過程で機能合成バグが見つかる可能性が高く、それらは個別の修正プラン（または
通常監査）で扱う。

## 計画化候補（plan 005 の実行で表面化した継ぎ目）

- **パーサの `while expr as T < expr` 誤判定** — plan 005 実行中に発覚。`while i as i64 < n {`
  の `< n` がジェネリック型引数の開始と解釈され parse 失敗する。strncmp は `let limit = n as i32`
  で回避済み（健全）。`as` キャストの右に `<` 比較が続く一般パターンに影響する潜在バグ。レバレッジ
  低〜中（回避容易だが書き心地の罠）。
- **`-nostdlib` 単体ではリンクできない（prelude `string.concat` が常時 IR に出る）** — plan 005 の
  libc 非依存テストは `-ffunction-sections -Wl,--gc-sections` でリンカ dead-code 除去を有効化して
  初めて通る。`string.concat`（malloc/strcpy/strcat 依存）が未参照でも IR に出るため。索引代入
  codegen を入れて `concat` を iris 化すれば GC フラグ不要で真の libc 全排除に到達（既に下の候補と
  STATUS に依存を記録済み）。

## 計画化候補（plan 002 の実行で表面化した機能の継ぎ目）

- **`Vec.pop` ／ 索引代入 `v[i] = x` の codegen** — plan 002 の初回 RPN showcase が必要とした
  が、いずれも未実装（`Vec.pop`: STATUS.md:246,345／索引代入: codegen「この代入先はコード生成
  に未対応です」）。このため**縮む／上書きできるスタックが書けず**、一般の RPN 評価器が正しく
  書けない。本来の RPN スタック版 showcase を実現するなら、どちらか（推奨は索引代入＝より汎用）
  を実装する別プランが要る。レバレッジ中（言語の表現力を広げ、第 2 showcase の前提になる）。

## 監査で挙がったが計画化しなかった項目

- **codegen の `unwrap()`/`unreachable!()`（内部不変条件）** — `codegen.rs:2488,2618,2627,2692,2437`。
  現状はすべて typeck の不変条件で守られ到達不能。将来 typeck が新パターンを通すと診断ではなく
  panic になる、という防御的ハードニング。レバレッジ低のため見送り（必要なら計画化可能）。

## 却下（再監査不要）

- **移植性／プラガブル syscall 層の即時実装**: 第 2 ターゲットが存在しないため 1 実装の抽象
  ＝YAGNI。実装はせず保留判断を plan 006（ADR-0014）として記録するに留めた。
- **フォーマッタ／LSP 等のツーリング**: 言語仕様がまだ縦切りで動いており不安定。「広まって
  ほしい」価値はあるが時期尚早。言語コアが固まってから再検討。
- **既知のメモリリーク**（Vec 引数・ループ本体 Vec の未解放、ヒープ `concat` のリーク） —
  `STATUS.md:246-248` に「健全だが…リーク」と明記された設計上のトレードオフ。バグではない。
- **`typeck.rs:1709/1740/1921` の `unwrap()`** — 直前の長さ検査で守られている。
