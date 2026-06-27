# 001 — 非 enum/bool scrutinee の match を catch-all 必須にする（未初期化読み出しの UB を塞ぐ）

- **対象コミット**: `34e8939`（作業ツリーには bool/数値 exhaustiveness の未コミット変更あり。本計画はその変更の**直後の状態**＝下記「現状コード」を前提とする）
- **カテゴリ**: 正しさ / 健全性（soundness）
- **見積**: S〜M（実装 ~25 行、テスト 3〜4 件）
- **リスク**: LOW（既存テストはすべて catch-all 付き＝壊れない。確認済み）

## なぜ重要か（背景）

`match` 式の exhaustiveness（網羅性）検査は現状 **enum / bool / 数値型のみ**を検査し、
それ以外の scrutinee 型（特に **`string`**、ほか `char` 等）を**素通り**させる。

一方 codegen（`src/codegen.rs` の `gen_match`）は match の結果を**初期化していない
`alloca`（result slot）**に各アームが `store` する方式で、全アームの条件が外れると
`merge` ラベルへ合流して **その未初期化スロットを `load` する**（`src/codegen.rs:2715` 付近）。

したがって次のような**網羅的でない文字列 match を式（値）位置で使う**と、型検査を
通過し、実行時にどの分岐にも当たらなかった場合に**未初期化メモリを読む（UB・ゴミ値）**:

```iris
fn rank(s: string): i32 {
    let r = match s {        // どのアームにも当たらないと未初期化スロットを load
        "gold" -> 3
        "silver" -> 2
    }                        // ← catch-all（_）が無い。現状エラーにならない
    return r
}
```

**根本原因**は型検査側の exhaustiveness 検査が enum/bool/数値で**枝分かれして打ち切られて
いる**こと。enum と bool だけが値域を有限に列挙できる型なので、**それ以外のすべての型は
catch-all（`_` またはガード無しの識別子束縛）を必須にする**のが正しい一般化。数値型に
すでに同じルールがあり、それを「数値だけ」ではなく「enum/bool 以外すべて」へ広げる。

副次的に、現状の数値ブランチは **`or` パターンに包まれたワイルドカード**（例
`match n { 1 | _ -> ... }`）を catch-all と認識できず**誤って非網羅と報告する**バグがある
（`arm.pattern` を直接 `matches!(Wildcard | Bind)` で見ており `Or` を展開しない）。本修正で
catch-all 判定を `Or` 展開込みの共通ヘルパに統一し、これも同時に直す。

## スコープ

**変更してよいファイルは 2 つだけ**:

- `src/sema/typeck.rs` — exhaustiveness 検査の最後のブランチを置き換える（下記）
- `tests/typeck.rs` — 回帰テストを追加する

**触ってはいけない**:

- `src/codegen.rs`（result slot の未初期化は**症状**。根本は typeck で塞ぐ。codegen は変えない）
- enum / bool ブランチの既存ロジック（数値ブランチのみ一般化する）
- `std/*.iris`、`examples/*.iris`（`std/os.iris:126` の match は `Option`＝enum で網羅済み・影響なし）
- パターンの構文・パーサ・所有権・名前解決の各層

## 現状コード（`src/sema/typeck.rs`、修正対象）

exhaustiveness 検査の末尾はこうなっている（行番号は目安。`else if scrut_ty.is_numeric()`
ブロックが**置き換え対象**）:

```rust
        } else if scrut_ty == Ty::named("bool") {
            // bool: true/false 両方を literal で覆うか、catch-all で網羅。
            // ...（このブロックは変更しない）...
        } else if scrut_ty.is_numeric() {
            // 数値型は値域が無限なので catch-all（`_` または識別子束縛）が必須。
            let has_catchall = arms.iter().any(|arm| {
                arm.guard.is_none()
                    && matches!(&arm.pattern, Pattern::Wildcard { .. } | Pattern::Bind { .. })
            });
            if !has_catchall {
                self.error(
                    span,
                    format!(
                        "match が網羅的ではありません: 数値型 `{}` は値域が無限のため `_` または識別子束縛が必要です",
                        scrut_ty.describe()
                    ),
                );
            }
        }
```

## 実装手順

### ステップ 1 — 数値ブランチを「enum/bool 以外すべて」の catch-all 必須ブランチへ一般化

`src/sema/typeck.rs` の `else if scrut_ty.is_numeric() { ... }` ブロック**全体**を、次の
`else if`（Error/Infer を除外した catch-all 必須ブランチ）へ置き換える:

```rust
        } else if scrut_ty != Ty::Error && scrut_ty != Ty::Infer {
            // enum / bool 以外（数値・string・char・struct 等）は値域を有限に列挙
            // できないので、catch-all（`_` またはガード無しの識別子束縛）が必須。
            // これが無いと codegen が初期化されていない result slot を load する（UB）。
            // Error / Infer の scrutinee（先行する型エラー由来）には波及エラーを出さない。
            let has_catchall = arms.iter().any(|arm| {
                if arm.guard.is_some() {
                    return false;
                }
                // `or` パターンは展開して、どれか 1 つが catch-all なら catch-all 扱い。
                let pats: &[Pattern] = match &arm.pattern {
                    Pattern::Or { patterns, .. } => patterns,
                    p => std::slice::from_ref(p),
                };
                pats.iter().any(|p| {
                    matches!(p, Pattern::Wildcard { .. } | Pattern::Bind { .. })
                })
            });
            if !has_catchall {
                self.error(
                    span,
                    format!(
                        "match が網羅的ではありません: 型 `{}` は catch-all（`_` または識別子束縛）が必要です",
                        scrut_ty.describe()
                    ),
                );
            }
        }
```

注意点:
- `Ty::Error` / `Ty::Infer` の除外は**必須**。先行する型エラーで scrut_ty が `Error` になった
  match に網羅エラーを重ねて出さないため（波及エラー抑制）。
- 非 enum の scrutinee では `Bind`（識別子）は必ず catch-all（バリアント名ではありえない）。
  enum ブランチのような「束縛名がバリアントか」の分岐は**不要**。
- メッセージは数値固有の「値域が無限」をやめ汎用文言にする。既存テストは `"網羅"` 部分文字列
  で照合しているので壊れない（下記テストで確認）。
- `Pattern` / `LitPat` は同ファイル先頭で既に `use` 済み（既存ブランチが使用している）。新規 import 不要。

### ステップ 2 — 回帰テストを追加

`tests/typeck.rs` の**末尾**（既存の `accepts_exhaustive_numeric_match_with_bind` の後ろ）へ
以下を追記する。ヘルパ `typecheck(src) -> Result<(), Vec<String>>` は同ファイル先頭で定義済み。
既存テスト（例 `rejects_non_exhaustive_enum_match`）と同じ書式に揃えること。

```rust
#[test]
fn rejects_non_exhaustive_string_match() {
    // 文字列 match に catch-all が無い → 網羅エラー（未初期化読み出しの UB を型検査で塞ぐ）。
    let src = "fn r(s: string): i32 {\n    let x = match s {\n        \"gold\" -> 3\n        \"silver\" -> 2\n    }\n    return x\n}";
    let errs = typecheck(src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("網羅")), "got: {errs:?}");
}

#[test]
fn accepts_exhaustive_string_match_with_wildcard() {
    let src = "fn r(s: string): i32 {\n    match s {\n        \"gold\" -> 3\n        _ -> 0\n    }\n}";
    typecheck(src).expect("ワイルドカードで網羅");
}

#[test]
fn accepts_exhaustive_string_match_with_bind() {
    let src = "fn r(s: string): i32 {\n    match s {\n        \"gold\" -> 3\n        rest -> 0\n    }\n}";
    typecheck(src).expect("識別子束縛で網羅");
}

#[test]
fn accepts_exhaustive_numeric_match_with_or_wildcard() {
    // 副次バグの回帰: `or` に包まれた `_` を catch-all と認識する。
    let src = "fn f(n: i32): i32 {\n    match n {\n        0 -> 0\n        1 | _ -> 9\n    }\n}";
    typecheck(src).expect("or 内のワイルドカードで網羅");
}
```

## 完了条件（機械的に検証可能）

1. ビルドが通る:
   ```sh
   cargo build 2>&1 | tail -5
   ```
   → エラー 0（`warning` は可）。

2. 追加した 4 テストが通り、既存テストが**1 件も**壊れない:
   ```sh
   cargo test --test typeck 2>&1 | tail -5
   ```
   → `test result: ok.` で `0 failed`。テスト総数は現状 +4。

3. 全テストスイートが緑のまま:
   ```sh
   cargo test 2>&1 | grep -E 'test result|error\[' | grep -v 'ok\.'
   ```
   → 出力が空（`ok.` 以外の `test result` 行や `error[` が無い）。

4. 手動スポット確認（任意）— 修正前は通っていた非網羅文字列 match が今はエラーになる:
   ```sh
   printf 'fn r(s: string): i32 {\n    let x = match s {\n        "a" -> 1\n    }\n    return x\n}\nfn main(): i32 { return r("a") }\n' > /tmp/nonexh.iris
   cargo run -- --emit-llvm /tmp/nonexh.iris 2>&1 | grep -i '網羅'
   ```
   → 「match が網羅的ではありません」を含む診断が出る（修正前は IR が出力されていた）。

## テスト方針

- 新規テストは `tests/typeck.rs`（型検査の回帰）に置く。codegen 側のテストは不要
  ——根本修正は型検査で、不正プログラムは codegen に到達しなくなるため。
- 既存の enum / bool / 数値 exhaustiveness テスト（`tests/typeck.rs:417` 以降）が
  リグレッションの番人。触らない。

## メンテナンスノート（レビュー時に見るべき点）

- 将来 **新しい有限列挙型**（例: 値域の小さい新プリミティブ）を追加して「全値列挙で網羅」
  を許したくなったら、enum/bool と同様の専用ブランチを**この catch-all ブランチより前**に
  追加する。後ろに置くと汎用 catch-all 必須ルールに先取りされる。
- codegen の result slot 未初期化（`src/codegen.rs:2715` 付近）は**この型検査ルールが
  唯一の防壁**。`gen_match` の生成方式を変えるときは、この不変条件（到達不能合流が
  無い）を壊さないこと。気になるなら別途「result slot をゼロ初期化 or `unreachable` で
  trap する」防御を入れてもよいが、本計画のスコープ外。

## エスカレーション（想定と違ったら止めて報告）

- もし**既存テストが新たに失敗**したら（＝どこかに catch-all 無しの非 enum match が
  正当な用途で存在する）、勝手に修正せず**停止して報告**。設計判断（その用途を許すか）が
  必要。
- もし `scrut_ty` に `Ty::Error` / `Ty::Infer` 以外の「網羅エラーを出すべきでない」型
  （例: `void`/unit の match）が実在し波及エラーが出たら、除外条件の追加要否を**報告**。
