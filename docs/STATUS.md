# 実装状況

iris-lang コンパイラの実装進捗。最終更新: 2026-06-23（`std/os.iris` の NULL 安全化＝組み込み述語 `is_null` と `open`/`env` の `Option` 戻りラッパ。`as` 型変換＝数値↔数値/`bool`→数値、`RawPtr` 不透明ポインタ＋`std/os.iris`＝FILE I/O・プロセス・環境変数）。

## パイプライン

```
ソース ─字句解析→ トークン列 ─構文解析→ AST ─名前解決→ 型検査→ 所有権DAG検査→ LLVM IR生成→ clang
       (lexer.rs)          (parser/)       (resolve)  (typeck)  (ownership/)   (codegen.rs)  (実行ファイル)
```

現状はフロントエンドの **字句解析 → 構文解析 → AST → 名前解決 → 型検査 → 所有権 DAG 検査** までを縦切りで実装済み。
さらに **LLVM IR（テキスト `.ll`）生成**に着手済み（数値プリミティブ `i8..u64` / `f32` / `f64`、`bool`、参照、struct、文字列）。`clang` で実行ファイル化できる。
エラーはすべて [miette](https://github.com/zkat/miette) でソース位置付きで表示する。
字句・構文解析には [nom 8](https://github.com/rust-bakery/nom)（+ nom_locate）を用いる。

## ファイル構成

| ファイル | 役割 | 状態 |
|---|---|---|
| `src/span.rs` | ソース位置（offset/len）。miette `SourceSpan` へ変換 | ✅ |
| `src/token.rs` | トークン定義 | ✅ |
| `src/ast.rs` | 抽象構文木 | ✅（縦切り範囲） |
| `src/lexer.rs` | nom + nom_locate による字句解析（span付き） | ✅（縦切り範囲） |
| `src/parser/tokens.rs` | `&[Token]` への nom `Input` 実装 | ✅ |
| `src/parser/error.rs` | 構文解析エラー型 | ✅ |
| `src/parser/mod.rs` | 構文解析（式は優先順位ごとに段分け） | ✅（縦切り範囲） |
| `src/sema/resolve.rs` | 名前解決（スコープ構築・定義/使用の対応） | ✅（縦切り範囲） |
| `src/sema/ty.rs` | 型検査の内部型表現 `Ty` | ✅（縦切り範囲） |
| `src/sema/typeck.rs` | 型検査（型付け・整合性検査・可変性検査） | ✅（縦切り範囲） |
| `src/sema/ownership/` | 所有権 DAG（型グラフ循環検出＋ムーブ/借用グラフ＋借用競合） | ✅（縦切り範囲） |
| `src/codegen.rs` | LLVM IR（テキスト）生成 | ✅（数値プリミティブ / bool / 参照 / struct / 文字列） |
| `std/std.iris` | 最小の標準ライブラリ（iris 自身で記述・自動前置） | ✅ |
| `src/diagnostics.rs` | miette 診断 | ✅ |
| `src/lib.rs` / `src/main.rs` | ライブラリ / CLI（AST 表示・`--emit-llvm`・`build`/`run`） | ✅ |
| `tests/parse.rs` | 字句・構文解析の回帰テスト | ✅ |
| `tests/resolve.rs` | 名前解決の回帰テスト | ✅ |
| `tests/typeck.rs` | 型検査の回帰テスト | ✅ |
| `tests/typedef.rs` | 型定義・構造体リテラルの回帰テスト | ✅ |
| `tests/ownership.rs` | ムーブ検査の回帰テスト | ✅ |
| `tests/ownership_dag.rs` | 所有権 DAG 循環検出・ライフタイムの回帰テスト | ✅ |
| `tests/borrow_conflict.rs` | 借用競合（エイリアス規則）の回帰テスト | ✅ |
| `tests/methods.rs` | 固有メソッド `impl`・self 三形・ドット呼び出しの縦断回帰テスト | ✅ |
| `tests/traits.rs` | trait 定義・`impl Trait for`・適合・`#`・既定実装・スーパートレイト・境界/単相化・構造的 `==` の縦断回帰テスト | ✅ |
| `tests/codegen.rs` | LLVM IR 生成・clang 実行の回帰テスト | ✅ |
| `tests/std_io.rs` | std を使った出力プログラムの実行テスト | ✅ |
| `tests/module.rs` | モジュール use・pub 可視性・モジュールパス呼び出しの回帰テスト | ✅ |
| `src/module.rs` | モジュールローダー（ファイル読み込み・pub アイテム抽出・パス解決） | ✅ |
| `std/prelude.iris` | 最小 std（旧 std/std.iris からリネーム） | ✅ |
| `std/os.iris` | OS モジュール（`use std.os`。FILE I/O・プロセス・環境変数） | ✅ |

## 構文の実装状況

### 実装済み

- 関数定義 `fn name(params): RetType { ... }`（`pub`、戻り値型省略可、末尾カンマ可）
- 外部関数宣言 `extern fn name(params): RetType`（本体なし・C 関数を借りる。例: `putchar`）
- 型定義 `type Name<T> = (別名 | struct { ... } | enum { ... })`（`pub`、ジェネリクス、フィールド/バリアントは改行・カンマ区切り）。enum バリアントのペイロード型指定は `Name: Type`（コロン）と `Name(Type)`（丸括弧）の両形式に対応
- 固有メソッド `impl Type { fn m(self / &self / &mut self, ...): Ret { ... } }`（トレイト無し）。
  ドット呼び出し `x.m(args)` を静的ディスパッチで解決。`self` の三形（値＝ムーブ・`&self`＝
  共有借用・`&mut self`＝可変借用）に対応。`impl` は予約語ではなく識別子として扱う。
- **トレイト** `trait Name<T>: Super { fn sig [既定実装] }`・`impl Trait for Type`・
  ジェネリック境界 `fn f<T: Bound>(...)`・匿名境界 `x: A + B`・呼び出し修飾子 `x.m#Trait<Args>()`
  （ADR-0004〜0009、下記）。`trait` も識別子として扱う
- 構造体リテラル `Name { field: value, ... }`（`if`/三項の条件位置では抑制し曖昧性回避）
- 文: `let` / `const`、`return`、再代入 `target = value`、式文
- ループ: `while cond { ... }` / `loop { ... }` / `break` / `continue` / **`for x in lo..hi { ... }`**（整数範囲。`..` 排他・`..=` 包含）/ **`for x in iter { ... }`**（一般イテレータ＝`Iterator<T>` トレイト経由、ADR-0007。`iter` が `Iterator` を実装する値、要素型 `T` を `x` に束縛）/ **`for x in coll { ... }`**（配列 `T[]`・動的配列 `Vec<T>` の直接反復。要素は Copy 型限定・コレクションは借用＝消費しない）
- **配列リテラル `[e1, e2, ...]`**（改行/カンマ区切り・末尾カンマ・空リスト可）。型注釈に応じて固定長配列 `T[]`（スタック裏付け）または動的配列 `Vec<T>`（`malloc` でヒープ確保）を構築する（型指向。整数リテラル既定 `i32`）
- 型: 名前付き型、ジェネリクス `Vec<T>`、参照 `&T` / `&mut T`、配列 `T[]`、タプル `(A, B)`
- 式（優先順位対応）:
  - リテラル（整数・浮動小数点・文字列・真偽値）、識別子
  - 二項演算 `+ - * / %`、比較 `< <= > >=`、等価 `== !=`、論理 `&& ||`
  - 単項 `-`、参照 `&` / `&mut`
  - 後置: 関数呼び出し `f(...)`、メンバアクセス `a.b`、エラー伝播 `expr!`、**添字アクセス `base[index]`**（後置式・チェーン可。`string`→`u8`、固定長配列 `T[]`→`T`、`Vec<T>`→`T`。添字は任意の整数式）
  - 三項演算子 `cond ? a : b`
  - if 式（`else if` / `else` 連鎖）
  - **match 式** `match expr { Pattern [if guard] -> expr ... }`（アームは改行またはカンマ区切り。`->` は既存の `Arrow` トークンを流用）。パターンはワイルドカード `_`・リテラル（整数・浮動小数・bool・文字列）・**数値範囲 `1..10`（排他）/ `1..=10`（包含）**・enum バリアント（束縛あり）。各アームに**ガード `if cond`**（bool・束縛変数参照可）を付けられる
- 改行による文区切り、空白・タブ・行コメント `//` の読み飛ばし
- **関数本体の末尾式の暗黙 return**: `return` キーワードなしで最後の式が戻り値になる（`match` 式など）。両分岐とも `return` する `if-else` が末尾の場合も正しく処理する（到達不能な空の merge ブロックを生成しない）
- 字句エラー・構文エラーの miette 表示（該当 span を指す）

### 意味解析（名前解決のみ実装済み）

`src/sema/resolve.rs`。スコープ（グローバル / 関数 / ブロック）を構築し、識別子の
使用位置を定義（関数・引数・ローカル変数）に結びつける。

- 関数名は先に一括登録するため、相互再帰・前方参照が可能
- 検出するエラー（複数件をまとめて miette で表示）:
  - 未定義の名前の使用
  - トップレベル関数名の重複定義
  - 同一関数内での引数名の重複
- ローカル変数の `let` 再宣言（シャドーイング）は許可
- `Ok` / `Err` / `Some` / `None` をプレリュード（組み込み名）として登録
- 型名・メンバ名・構造体リテラルのフィールド名の検証は型情報が必要なため型検査側で行う（名前解決は値の名前のみ）

### 型検査（実装済み）

`src/sema/ty.rs`（内部型 `Ty`）・`src/sema/typeck.rs`。名前解決の結果を前提に
式・文・関数へ型を付け、整合性を検査する。

- プリミティブ（`i8`..`u64`, `f32/f64`, `bool`, `string`, `char`, `void`）と
  ジェネリクス（`Vec<T>`, `Result<T,E>`, `Option<T>` など）を扱う
- 整数・浮動小数リテラルは未確定型として任意の整数/小数型へ適合。注釈なし `let`
  では既定型（`i32` / `f64`）へ確定
- 検出するエラー（複数件をまとめて miette で表示）:
  - 代入・引数・戻り値・`let` 注釈の型不一致
  - 算術・比較（数値）/ 論理（bool）演算の被演算子型
  - 関数呼び出しの引数個数・型
  - `!` を Result/Option 以外へ適用、または Result/Option を返さない関数内での使用
  - 不変な束縛（`let`（mut なし）/ `const`）への再代入
  - `while` の条件が bool でない、`break`/`continue` のループ外使用
  - `for x in lo..hi` の範囲境界が整数でない／下限と上限の型不一致。ループ変数 `x` には境界の具体整数型（両方リテラルなら既定 `i32`）を付与し、本体内で使えるようにする
  - `for x in iter`（一般イテレータ）: `iter` の型が `Iterator<T>` を実装しているか検査し、要素型 `T` をループ変数 `x` に付与（`Iterator` 未実装なら拒否、複数 `Iterator` 実装の同居は当面曖昧エラー）。要素型は codegen が `next()` の戻り `Option<T>` のアンラップに使う
  - `for x in coll`（配列 `T[]` / 動的配列 `Vec<T>`）: 要素型 `T` を `x` に付与し、要素を直接反復する（`Iterator` 実装より優先）。当面 `x` は **Copy 型のみ**（非 Copy 要素は拒否）。コレクションは借用（消費しない）
  - **配列リテラル `[...]`** は未確定型 `ArrayLit(要素型)` として型付けし、注釈（`T[]` / `Vec<T>`）に応じて確定する（型指向。`let` 注釈・戻り型・`for` の対象で `finalize_array_lit` が `expr_types` を書き換え、codegen が表現を決める）。注釈がなければ既定で固定長配列 `T[]`。要素型は全要素を `join` して求める（不一致はエラー）
- `&mut T` は `&T` として使える（参照の可変性は所有権パスで詳細検査）

#### 型定義の扱い（実装済み）

- 型名の検証（未定義の型名を報告）。既知 = プリミティブ＋組み込み（Vec/Result/Option/Box/Map/Set）＋定義済み型名
- `type` 別名は名前的型付け（別の型）。ただしリテラル代入の可否は別名の元の型で判定（`type Meters = f64` に `5.0` は可、`f64` 値は不可）
- struct: メンバアクセス `a.b` のフィールド型付け（参照は自動デリファレンス）、構造体リテラルのフィールド検査（型不一致・重複・未初期化・未知フィールド）
- 固有メソッド: 型名→メソッド名の表を作り、`x.m(args)` を `Member`+`Call` から静的に解決。
  受け手の型（参照は剥がす）でメソッドを引き、self を除く引数の個数・型を検査する。`&mut self`
  は可変な受け手（`let mut`・`&mut` 越し）を要求し、不変束縛・`&T` 越しの呼び出しを拒否する。
  未知メソッド・関連関数（self なし）の値経由呼び出しも報告する。メソッド表は impl に書いた
  型名そのもので引くため、`type Alias = Struct` の別名値からの呼び出しは現状解決しない（要 exact 名一致）。
  同じ型に同名メソッドがあれば（同一 impl・別 impl を問わず）**二重定義**として型検査で報告する
  （codegen に渡る前に止め、`@Type.method` 記号の衝突＝clang の再定義エラーを防ぐ）
- **enum**: 定義・型名登録・バリアント構築・`match` 式によるアンラップを実装済み。**ジェネリック enum（ビルトイン `Option<T>`/`Result<T,E>` ＋ ユーザ定義 `type Pair<T>` 等）を、スカラ／参照／`string`／**struct 等の集約ペイロード**まで縦断対応**（ADR-0010）
  - **バリアント構築**: ペイロードなし（`Red` → `Ident` ノード）とペイロードあり（`Rect(5)` → `Call` ノード）の両形式。パーサは通常の `Ident`/`Call` として出力し、typeck が `variant_owners` マップ（`バリアント名 → (enum 名, タグ index, ペイロード型, enum の型パラメータ名)`）を引いて enum 構築と判定。判定結果は `TypeInfo::variant_constructions`（式 span → `(enum 名, タグ, has_payload)`）として codegen へ渡す（AST を書き換えない設計）
  - **ジェネリック enum の単相化**（ADR-0010）: codegen はペイロードが i64 に収まるかでレイアウトを二分する。**スカラ**（数値・bool・参照・`string`＝ptr）は全インスタンス共通の `%Enum = type { i8, i64 }`（型引数はレイアウトに影響しない＝マングル型を出さない、i64 キャスト格納）。**集約**（struct 等）は per-instantiation の `%Enum.Args = type { i8, <最大サイズのペイロード型> }`（opaque ポインタにより typed store/load で bitcast 不要）。使用インスタンスは式の型と非ジェネリックなシグネチャから収集して型宣言を発行。typeck はバリアント構築でペイロード引数から型パラメータを `unify` で推論し（`First(5)` → `Pair<i32>`）、`match` では scrutinee の型引数で各バリアントのペイロード型を `subst` して束縛変数へ具体型を付与する。ビルトイン `Option`/`Result` は `prelude_variant`（typeck）と `StructReg.enum_layouts`（codegen、enum レイアウトの唯一の真実源）に固定タグ（None=0/Some=1、Ok=0/Err=1）で登録
  - **`match` パターン**: ワイルドカード `_`、enum バリアント名（`Red`）、バリアント＋束縛変数（`Rect(side)` ← ペイロードを取り出してアーム本体スコープへ束縛）、整数・浮動小数・bool・文字列リテラル（**文字列は codegen で `strcmp` 比較を実装済み**）、**数値範囲 `lo..hi`（排他）/ `lo..=hi`（包含）**（境界は整数・浮動小数リテラル）。各アームに**ガード `if cond`** を付けられる
  - **`match` の型検査**: scrutinee の型を確認し、各バリアントパターンが enum に存在するか検証。束縛変数には対応バリアントのペイロード型を付与。範囲パターンは下限・上限が同一数値クラスかつ scrutinee に適合するか検査。ガードは bool 型を要求（束縛変数を参照可）。全アームの結果型を `join` して `match` 式の型を決定
  - **名前解決**: enum のバリアント名（ジェネリック含む）はグローバルスコープへ `DefKind::Builtin` として登録。バリアント束縛変数はアームごとの独立スコープへ `DefKind::Local` として登録。ガードはアームスコープ内で解決（束縛変数を参照できる）
  - **所有権**: scrutinee はムーブ扱い。各アームは `if` の分岐と同様に独立した状態で評価し、全アームのムーブ集合を合流（flow.rs）。ガードは本体より先に読み取りとして評価。借用競合は各アームをブロックスコープとして扱い解放し、ガードもそのスコープ内で訪問（borrows.rs）
- ジェネリック struct は構造体リテラルのフィールド値から型パラメータを `unify` で推論し（`Pair { a: 1, b: 2 }` → `Pair<i32>`）、メンバアクセスはインスタンスの型引数でフィールド型を単相化する（`p.a: i32`）。フィールド値の型不一致（同じ `T` のフィールドに別型）も検出する

### 所有権 DAG（実装済み）

`src/sema/ownership/`（`typegraph.rs` / `flow.rs` / `borrows.rs` / `mod.rs`）。仕様の
所有権 DAG（`ownership.md` / `compiler.md` の Open Question）を三層で検査する。

**1. 型レベルの所有権グラフ＋循環検出**（`typegraph.rs`）
- 型同士の所有関係を有向グラフ化し、所有のサイクル（＝無限サイズ型）を検出
- 所有辺を作らない: 参照 `&T`（非所有）、`Box`/`Vec`/`Map`/`Set`（ヒープ間接）
- 所有辺を作る（インライン格納）: 名前付き型、`Option`/`Result`/タプル/固定長配列 `T[]`、別名
- 例: `me: Bad`（直接）→ 循環エラー、`kids: Vec<Tree>` / `parent: &Tree` → OK

**2. 値レベルのムーブ＋借用グラフ（ライフタイム）**（`flow.rs`）
- 既定はムーブ。値を関数引数・`let`・`return`・構造体リテラルのフィールドへ「値として」渡すとムーブ
- `Copy` 判定: 数値プリミティブ・`bool`・`char`・不変参照 `&T`・要素が全て Copy のタプル。別名はもとの型で判定（`type Meters = f64` は Copy）
- `&x` / `&mut x` は借用（ムーブしない）。固有メソッド呼び出し `x.m()` の受け手は、メソッドの
  self の受け方で決まる: `self`（値）は受け手をムーブ、`&self`/`&mut self` は借用。借用競合層では
  `&mut self` 呼び出しを受け手への可変借用イベントとして扱い、生存中の借用との競合を検出する
- 参照の **auto-deref 読み**はムーブしない: `&mut T` は非 Copy だが、算術・比較・論理・単項マイナス・条件のオペランドのように参照先の値が読まれる文脈では、参照値自体は使われずデリファレンス読み（借用）として扱う。これにより `r = r + 1`（`r: &mut T`、右辺で r を読みつつ write-through）が通る
- ムーブ済みの値の使用・借用を報告（ムーブ位置も副ラベルで表示）。再代入で再初期化。`if`/三項の分岐はムーブ集合を保守的に合流
- **ライフタイム検査**: 参照の出所（provenance）を借用グラフで追跡し、到達可能性で判定
  - ダングリング返却: ローカル／値渡し引数を指す参照を返すとエラー。参照引数経由（呼び出し側所有）は安全
  - 指す先のムーブ: 参照先がムーブされると、その参照の以降の使用をエラー

**3. 借用競合チェック（エイリアス規則）**（`borrows.rs`）
- 同一の場所に対し `&mut` は排他、`&` は複数可（`&mut` 生存中は他の借用不可）
- 借用の生存期間は**スコープベース**（保守的）: `let r = &x` の借用は宣言ブロックの終わりまで、式中の一時借用 `f(&mut x, &x)` はその文の間
- 検出: 二重 `&mut`、`&mut` と `&` の同時、同一呼び出し内の競合。別スコープ・別の場所・共有複数は OK。借用変数の再代入で解放
- 健全（実際の競合は見逃さない）だが、NLL のような最後の使用に基づく精密な生存期間ではないため保守的に多めに報告しうる

**ループ内のムーブ・借用**（`flow.rs` / `borrows.rs`）
- `for x in lo..hi` は `while` / `loop` と同じループ本体解析を適用する。範囲境界 `lo`/`hi` は読み（整数 Copy）として扱い、ループ変数 `x` は反復ごとに再束縛される整数 Copy（ムーブ追跡の対象外）
- ムーブ: 反復をまたぐ use-after-move を検出する。本体で生じるムーブを「静かな」先行パスで入口状態に合流させてから本パスを 1 回走らせる（ムーブ集合は単調なので 2 パスで安定）。`let` の再束縛はムーブ集合をリセットするため、反復ごとに作り直す値の移動は誤検出しない
- 借用: ループ本体を独立したブロックスコープとして扱い、反復ごとに借用を解放する（スコープベースのまま）

未対応: NLL 風の精密なライフタイム領域推論（最後の使用に基づく借用終了）、参照を返却以外の長寿命の場所へ格納する一般ケース、フィールド単位の部分ムーブ（メンバの値読みはオブジェクトの借用として寛容に扱う）

### LLVM コード生成（着手・部分実装）

`src/codegen.rs`。型検査・所有権検査を通った AST を **LLVM IR のテキスト（`.ll`）** へ
落とす。この環境には `llvm-config` が無く inkwell/llvm-sys が使えないため、まずは
テキスト出力とし、`clang file.ll -o out` で実行ファイル化する（JIT は将来 LLVM 導入時）。

- 対応（数値プリミティブと `bool`）: 関数定義・引数・再帰呼び出し、`let`/再代入/`return`、
  算術 `+ - * / %`、比較、論理 `&& ||`（短絡）、単項 `-`、`if` 文、三項演算子、
  ループ `while` / `loop` / `break` / `continue`（基本ブロック＋後方辺。`break`/`continue` はラベルスタックで解決）、
  **`for x in lo..hi`**（整数範囲をカウンタループへ落とす。下限を変数 `x` の場所へ格納し、上限を preheader で一度だけ評価、
  `cond`/`body`/`step`/`end` の 4 ブロックで `x < hi`（包含なら `x <= hi`、符号は `num_kind` で `slt`/`ult`/`sle`/`ule` を選択）が
  成り立つ間反復。`continue` は増分 `step` へ、`break` は `end` へ分岐）、
  **`for x in iter`**（一般イテレータ・ADR-0007。`loop { match iter.next() { Some(x) -> body, None -> break } }` 相当へ脱糖。
  イテレータの可変借用ポインタをループ前に一度だけ求め、`head` で `iter.next()`（`@Type.next`、提供元ラベルは `Iterator`）を呼んで
  `Option<T>` を退避、タグが `Some`（=1）なら要素を `x` に束縛して `body`、`None` なら `end` へ。`continue` は `head`（next 再呼び出し）へ、
  `break` は `end` へ。要素型 `T` は typeck の `for_iter_elem`、Some ペイロードのアンラップは `load_enum_payload` を共用し、
  スカラ／集約（struct 要素＝`Option<Point>` 等）の双方に対応）
- **数値型**: 整数 `i8..u64`（符号付き/なしで `sdiv`/`udiv`・`icmp slt`/`ult` 等を選択）と浮動小数 `f32`/`f64`
  （`fadd`/`fsub`/`fmul`/`fdiv`/`frem`・`fcmp o*`・`fneg`）。LLVM 型は符号を持たないため数値クラス（`NumKind`）を
  iris の `Ty` から導いて命令を選ぶ。浮動小数リテラルは double ビット列（`0x...`）で出力
- ローカルは alloca + load/store（SSA 化は LLVM の mem2reg に委ねられる）
- **struct**: 名前付き LLVM 構造体型 `%Name = type { ... }` を宣言。**ジェネリック struct は
  per-instantiation で単相化**（`type Pair<T> = struct {...}` → 使用ごとに `%Pair.i32 = type {...}`、
  記号は `mono_symbol`、フィールド型は型引数で置換。使用インスタンスは式の型・非ジェネリックな
  シグネチャから収集し、入れ子のジェネリック struct・集約 enum も辿って宣言）。typeck は構造体
  リテラルのフィールド値から型パラメータを `unify` で推論（`Pair { a: 1, b: 2 }` → `Pair<i32>`、整数/
  小数リテラルは既定 `i32`/`f64` へ確定）。`StructReg.struct_layout_of` が非ジェネリック（正規名）と
  ジェネリックインスタンス（単相化記号＋置換フィールド）を統一的に解決し、構築・フィールドアクセス・
  構造的 `==` で共用する。
  構造体値は first-class 値として扱う（関数引数・戻り値・`let` で値渡し）。
  - 構造体リテラル `Name { ... }`: alloca → 各フィールドへ `getelementptr` + `store` → 全体を `load`
  - メンバアクセス `a.b`: フィールドの `getelementptr` から `load`（ネストした `a.b.c` のチェーン、
    参照越し `(&a).b`、関数戻り値など場所でない値からのアクセスにも対応＝一時 alloca へ退避）
  - フィールド代入 `a.b = v`（ローカル struct への書き込み）
  - `type X = Y` の別名は struct/プリミティブ双方とも末尾までたどって解決する
- **固有メソッド**: メソッドは `@Type.method` の通常関数として emit する（self を第一引数に合成）。
  `self`（値）は struct を値渡し、`&self`/`&mut self` は受け手のアドレス（`ptr`）を渡す。受け手が
  参照値ならそのポインタを、場所ならアドレスを、それ以外は一時 alloca へ退避して渡す。ドット
  呼び出し `x.m(args)` は受け手の型名から記号を引いて直接 `call` する。`&mut self` のフィールド
  書き換えは参照越しに反映される（呼び出し側から観測できる）
- **参照 `&T` / `&mut T`**: opaque ポインタ（`ptr`）で表現。`&x` / `&mut x` は場所（ローカル/引数の alloca）のアドレスを値として返す。値が期待される文脈（算術・比較・条件・引数・`return`・注釈付き `let`）では**暗黙にデリファレンス**（`load`）して指す先の値を取り出す。多段参照も剥がす。typeck 側も `&T` を `T` の位置で受け入れる（`assignable` / `join_numeric` / `expect_bool` で参照を剥がす）。**参照越しの代入（write-through）に対応**：代入先が `&mut T` で右辺が値型 T のとき、参照値（`ptr`）を load してその指す先へ `store` する（`&T` の参照先への書き込みは型検査で拒否）。右辺が参照型のときは従来どおり束縛の付け替え（rebind）。typeck / flow（r をムーブせず借用として使用し provenance を保つ）/ borrows（write-through は借用を解放しない）の三層で整合
- **文字列 `string`**: C 風の **NUL 終端**表現。文字列リテラルは `[N x i8]` のグローバル定数
  `@.str.N = private unnamed_addr constant [N x i8] c"...\00"` にし、文字列値は opaque ポインタ
  `ptr`（先頭バイトのアドレス）として扱う（`llvm_ty("string") = ptr`、`zero_value(ptr) = null`）。
  リテラルの符号化は印字可能 ASCII 以外と `"` `\` を `\XX`（16進）でエスケープし末尾に NUL を付ける
  （マルチバイト UTF-8 はバイト単位）。値渡し（引数・戻り値・`let`）に対応。`extern fn puts`（std）で
  libc に渡して出力できる。文字列は **Copy 型**として扱う（不変な NUL 終端ポインタ。free/drop を持たず
  ポインタ複製は安全＝Rust の `&str` 相当）。**操作**: `s.len()`（libc `strlen` を借りる。`i32`）・
  添字 `s[i]`（i 番目のバイトを `u8` で返す＝`getelementptr i8` + `load i8`）・`a.concat(b)`（`malloc` で
  確保したバッファへ `strcpy`+`strcat`。結果はヒープ＝現状 free 無しでリークを許容、`Vec` と同様）を
  `std/prelude.iris` の `impl string` ＋ libc extern で実装済み。**残り**: 補間・スライス・`as` 変換
- **配列 `T[]` / 動的配列 `Vec<T>`**: 使用時のみ名前付き型を宣言する。固定長配列は fat pointer
  `%Array = type { ptr, i64 }`（データポインタ＋長さ）。配列リテラルは `alloca [N x T]` をスタックに
  確保し各要素を `store`、先頭アドレスと長さ `N` を `insertvalue` で組む。動的配列は
  `%Vec = type { ptr, i64, i64 }`（ポインタ＋長さ＋容量）。配列リテラルから `malloc`（`sizeof(T)` は
  `getelementptr T, null, 1` → `ptrtoint` で算出）でバッファを確保し要素をコピー、`len=cap=N`。
  要素型は opaque ポインタ越しに命令側で扱うため、型宣言は要素型に依らず単一（`%Array`/`%Vec`）。
  `for x in coll` はデータポインタと長さを `extractvalue` で取り出し、`0..len` のインデックスループへ
  落とす（`getelementptr` で要素アドレス→`load`→`x` のスロットへ `store`。`continue`/`break` はラベル
  スタックで解決）。**索引 `coll[i]`** はデータポインタを `extractvalue 0` で取り出し `getelementptr` +
  `load` で要素を読む（要素型は typeck の `expr_types` から、添字はネイティブ整数型のまま GEP 添字に使う）。
  malloc は `extern fn malloc(n: i32)`（prelude）として宣言し、`Vec` リテラルの確保もこれに合わせて i32
  引数で呼ぶ（x86-64 では i32 引数が rdi へゼロ拡張され size_t 互換。確保サイズは i32 へ trunc）。
  **所有権ベースの解放（Drop/free）**: ヒープ所有する `Vec<T>` ローカルを**スコープ末で `free` する**（`declare void @free(ptr)`）。
  各 Vec ローカルに**ドロップフラグ**（`i1` の alloca、entry で `false` 初期化・`let` 格納後に `true`）を持たせ、
  値が move された地点（値渡し引数・`return`・別束縛・struct フィールドへの格納＝裸の Vec 識別子の消費）で `false` に戻す。
  各 `ret` の直前で `emit_drops` がフラグの立つ Vec だけを `free`（`extractvalue 0` でデータポインタを取り出す）。
  **条件分岐の move も正確に追える**（Rust の動的 drop flag 相当。片方の分岐だけで move された値は実行時にフラグで解放可否が決まる）。
  借用 `&v`・添字 `v[i]`・`for x in v` は move でなく別経路で評価されるためフラグを落とさない（借用後も解放される）。
  **未対応**: `push`/`len`・スライス・非 Copy 要素。Drop の**スコープ粒度は関数末のみ**（ループ本体・ネストブロック単位の早期解放は未実装＝
  ループ内で確保した Vec は反復ごとに解放されず関数末まで生存＝健全だが反復分リーク）。Vec **引数**（値渡しで受け取った Vec）は
  callee で解放しない（＝リーク。健全）。struct フィールドの Vec の再帰 Drop も未実装（move 元のフラグは落とすので二重解放は無いが struct は解放されない）
- **enum**: 非ジェネリック・スカラペイロードのジェネリックは `%EnumName = type { i8, i64 }`（i8 = タグ、i64 = ペイロードの記憶域）。**集約ペイロードのジェネリック enum（`Option<Point>` 等）は per-instantiation の `%Enum.Args = type { i8, <記憶域> }`**（ADR-0010）。`StructReg.enum_layouts`（ビルトイン Option/Result ＋ ユーザ enum、generics 付き）が enum レイアウトの唯一の真実源で、`enum_tag`/`enum_payload_of`/`enum_is_aggregate`/`enum_storage_ty` を提供。`llvm_ty` は enum 名を、スカラなら `%Name`・集約なら `%Name.Args`（`mono_symbol`）へ写す。`gen_enum_construction`/`gen_match` は集約なら typed store/load、スカラなら i64 キャストを使う
  - **バリアント構築** `gen_enum_construction`: alloca → タグを `getelementptr` + `store i8` → ペイロードを `getelementptr` + `cast_to_i64` + `store i64` → `load %EnumName`。`cast_to_i64` は `i32`→`sext`、`bool`→`zext`、`f64`→`bitcast`、`ptr`→`ptrtoint` で i64 へ変換。typeck が `variant_constructions` に記録した span で `gen_expr` の先頭で命中したら `gen_enum_construction` へ分岐（AST ノード種を変えない）
  - **`match` 式** `gen_match`: 結果を受け取る alloca（result slot）を確保 → scrutinee を alloca へ退避 → `getelementptr` でタグフィールドを load → アームを順に if-else 連鎖でチェック（ワイルドカードは無条件 `br`）→ 各アームでペイロード束縛変数（`cast_from_i64` で元の型へ変換し alloca に退避）を用意 → アーム本体を評価して result slot へ store → `match.end` ラベルで合流 → result slot を load して値を返す。`cast_from_i64` は `trunc`/`fptrunc`/`inttoptr` 等で元の型へ戻す
    - **パターンの条件生成**: リテラルは `icmp eq`/`fcmp oeq`、enum バリアントはタグの `icmp eq`、**文字列は `call i32 @strcmp(...)` の結果を `icmp eq ..., 0`** で比較、**数値範囲は下限 `*ge` と上限 `*lt`/`*le`（排他/包含）を `and i1` で合成**（符号は `num_kind` で `s`/`u`/`fcmp o` を選択）
    - **ガード**: パターン一致後、束縛変数を設定してからガード式を評価し、`br i1 guard, %match.guarded, %skip`（不成立なら次アームのチェックへフォールスルー）。最終アームのガード不成立は exhaustiveness 未検査のため result slot 未初期化のまま合流する（保守的に許容）
- **struct**（上記。ジェネリック struct の単相化を含む）。**トレイト**（`impl Trait for Type` の
  メソッド emit・既定実装の合成・ジェネリック関数の単相化・構造的 `==`）は実装済み（上記トレイト節）
- **`!`（エラー伝播、ADR-0002）** `gen_try`: inner（Result/Option）を評価して alloca へ退避し
  タグを読む。成功タグ（Some=1 / Ok=0）と一致すれば継続ブロックでペイロードをアンラップして
  式の値とし、不一致なら伝播ブロックで**関数の戻り型へ `Err(e)` / `None` を再構築して早期 `ret`**
  （`gen_enum_value`／`load_enum_payload` を共用）。スカラ・集約（struct エラー）両ペイロードに対応。
  typeck は inner の種別が戻り型の種別（Result/Option）と一致し、Result では Err の型 E が
  戻り型の E と互換であることを検査。集約レイアウト整合のため、`return`／注釈付き `let` で
  enum 構築式の未確定型引数を文脈から埋める（`refine_construction`）
- `main(): i32` の戻り値が終了コードになり、`clang` で実行して検証できる
- CLI: `iris --emit-llvm <file>`（IR表示）/ `iris build [--release] [-o OUT] <file>`（実行ファイル生成）/ `iris run <file>`（即実行）
- **二段ビルド**: 既定 -O0（開発・高速）、`--release` で -O2。最適化の重さがビルド時間を支配するため、開発は -O0 既定にして速くしている（800関数で約9倍差）
- `extern fn` は `declare` を出力し、`clang` が libc をリンク（`putchar` 等が使える）
- **`as` 型変換**: `expr as Type` を数値↔数値・`bool`→数値で実装済み（`gen_cast`。`trunc`/`sext`/`zext`／`sitofp`/`uitofp`／`fptosi`/`fptoui`／`fptrunc`/`fpext`、LLVM 表現が同一なら無変換）。
- 未対応（今後）: 文字列補間・スライス（長さ `s.len()`・索引 `s[i]`→`u8`・連結 `a.concat(b)` は実装済み）（`!` エラー伝播・ジェネリック **enum**（集約ペイロード）＝ADR-0010・ジェネリック**関数**・ジェネリック **struct 型**の単相化・`as` 型変換はいずれも実装済み）。なお整数/小数リテラルからジェネリック struct を構築すると型パラメータは既定（`i32`/`f64`）に確定し、`Pair<i64>` 等の非既定幅の明示注釈は構築式へ伝播しない（型エラーになる。enum の `refine_construction` と同種の制約）

### 標準ライブラリ（最小・iris 自身で記述）

`std/prelude.iris`。`extern fn putchar` を借りて I/O を実現し、**iris 自身**で書いた最小の
ライブラリ。コンパイル時に各プログラムの先頭へ自動で前置される（単一の文字列として
連結し span を一意に保つ）。

- 提供: `putchar` / `puts` / `strcmp` / `strlen` / `malloc` / `strcpy` / `strcat`（extern）、`put_digit` / `newline` / `print_int` / `println_int` / `println_bool`、`trait Iterator<T> { fn next(&mut self): Option<T> }`（`for x in iter` が要求する標準トレイト・ADR-0007）、`impl string { fn len(&self): i32, fn concat(&self, other: string): string }`（libc を借りた文字列操作）
- `puts` は NUL 終端文字列を出力し末尾に改行を付ける（文字列リテラルの出力に使える）
- `strcmp` は libc から借りる文字列比較。`match` の文字列リテラルパターンの codegen が呼び出す（未使用でも `declare` のみ出力され無害）
- 現状のコード生成に合わせ `i32` / `bool` / `string` の範囲で記述
- これにより `main(): i32` から実際に数値・真偽値を標準出力へ表示し、`clang` でビルドして実行できる

#### `std/os.iris`（OS モジュール・実装済み）

prelude と違い**自動前置されず**、`use std.os`（または `use std.os.*`）で明示的に取り込む追加 std モジュール。
モジュール解決（`std.*` → `CARGO_MANIFEST_DIR/std/os.iris`）経由でロードされる。

- **不透明 C ポインタ型 `RawPtr`**: FFI 用のコンパイラ組み込みプリミティブ。LLVM では opaque ポインタ（`ptr`）で表現し、
  `string` と同様に free/drop を持たない **Copy** 型・所有グラフの辺を作らない。typeck（`BUILTIN_TYPES`・両 `is_copy`）と
  codegen（`llvm_ty` → `ptr`）に最小限で配線。std/os は `pub type File = RawPtr` と名前付けして使う（コンパイラは stdio を知らない）
- 生の extern（いずれも libc を `extern` で借りる）: `type File = RawPtr`、`fopen(path, mode): File` / `fclose(f): i32` /
  `fputs(s, f): i32` / `fgets(buf, n, f): string`（`buf` は `malloc` 確保の書き込み可能バッファ）、`exit(code): void`、`getenv(name): string`
- **NULL 安全な高水準 API（実装済み）**: `open(path, mode): Option<File>` / `env(name): Option<string>`。失敗（NULL）を `None`、
  成功を `Some(...)` に包んで返すため、利用側は `match` で安全に分岐でき生 NULL を見ない。生の `fopen`/`getenv` より推奨
- **組み込み述語 `is_null(p): bool`**: ポインタ裏付けの型（`RawPtr`/`string`、別名含む）が NULL かを返すコンパイラ組み込み。
  resolve（`PRELUDE`）→ typeck（`is_rawptr_like` で引数検査・`bool` 返り）→ codegen（`icmp eq ptr %p, null`）に配線。
  上記 `open`/`env` ラッパの土台。`null` リテラル構文は導入していない（NULL 判定はこの述語に集約）
- 書き込み→読み戻しのファイル往復、`open` の Some/None 両経路、`exit` による終了コード、`env` の設定/未設定を `clang` 実行で検証（`tests/codegen.rs`、`tests/typeck.rs`）
- **残り**: `fputs`/`fgets` 失敗（負値・NULL）の `Option`/`Result` 化、`stdout`/`stderr` グローバル（FILE\*）の参照、
  `fprintf` 等の可変長引数、`File` の自動 `fclose`（Drop）は未対応

### トレイトシステム（実装済み・ADR-0004〜0009）

`trait` 定義・`impl Trait for Type`・ジェネリック境界を、構文解析〜型検査〜所有権〜コード生成
（単相化して `clang` 実行）まで縦断実装した（`tests/traits.rs`）。

- **定義** `trait Name<T>: Super1 + Super2 { fn sig [既定実装] }`（`src/parser/`、`src/ast.rs`）。
  メソッドはシグネチャのみ／既定実装（本体付き）。`self` の型は抽象 `Self`。`#` は新トークン。
- **実装** `impl Trait for Type { ... }`。固有 `impl Type` と同じ構文を `for` の有無で判別。
- **適合検査**（`src/sema/typeck.rs` `check_conformance`）: トレイトの全メソッドを提供（既定実装
  があれば省略可）・シグネチャ一致（`Self`・トレイト型引数を実装型/指定引数へ置換して照合）・
  トレイトに無いメソッドの拒否・スーパートレイト実装の要求（ADR-0007）・同一キー
  `(Trait<引数>, Type)` の重複実装の拒否（スコープ・コヒーレンス、ADR-0008）。
- **メソッド解決**（ADR-0004）: 受け手が具象型なら固有＋実装トレイトの直接メソッド、ジェネリック
  型パラメータなら境界（スーパートレイトを辿る）から提供元を集め、ちょうど 1 個へ解決。複数なら
  曖昧エラーで `x.m#Trait<Args>()` / `x.m#Type()` を促す。固有 vs trait の同名も曖昧扱い（D2）。
- **既定実装**: impl が省略したメソッドは、実装型ごとに `Self → 実装型` で本体を合成して
  `@Type.method` を emit。既定本体は型検査では `Self` を「そのトレイトを実装する抽象型」として検査。
- **ジェネリック境界 `<T: Bound>` と単相化**（ADR-0006）: 関数の型パラメータと境界を解析。呼び出し時に
  引数から型引数を推論（`unify`）し、境界トレイトの実装を確認。コード生成は呼び出しごとに具体型で
  単相化した実体 `@f.Type` を出力（入れ子のジェネリック呼び出しもワークリストで閉包に含める）。
  匿名境界 `x: Greet + Serialize` はパーサが匿名ジェネリックパラメータへ脱糖（`impl Trait` 引数相当）。
- **構造的 `==` / `!=`**（ADR-0009）: struct は既定でフィールドを再帰比較し AND 合成（codegen の
  `gen_struct_eq`/`gen_eq_at`）。`==`/`!=` は所有権上は読みのみで被演算子を消費しない。
- 記号衝突対策: 固有と trait（または複数 trait）で同名メソッドが衝突する型は、固有を
  `@Type.method`・trait を `@Type.Trait.method` の別記号で emit し、`#` 修飾子で正しく振り分ける
  （typeck が解決済み提供元を span ごとに記録し codegen が参照）。

未対応（トレイト周辺の残り）: `dyn`／トレイトオブジェクト（ADR-0006 で当面持たない）、戻り値
`-> impl Trait`、ジェネリックな型自体への `impl`（`impl Trait for Box<T>`）、self なし関連関数の
`Type.func()` 呼び出し構文、`Eq` トレイトによる `==` の上書き（構造的既定のみ）、ジェネリック
メソッド経由の入れ子でない単相化に限定（具象型は generic でない前提）。

### 未実装

- ループ `for`: **整数範囲 `for x in lo..hi` / `lo..=hi`**・**一般イテレータ `for x in iter`（`Iterator<T>` 経由・ADR-0007）**・**配列 `T[]` / 動的配列 `Vec<T>` の直接反復 `for x in coll`** は実装済み（いずれも縦断・clang 実行）。**残り**: 非 Copy 要素の反復（要素のムーブ/借用の所有権設計）、複数 `Iterator` 実装の同居を解く `for x#T in iter` 修飾構文、浮動小数範囲
- `match` の拡張: リテラル（整数・浮動小数・bool・**文字列**）・**範囲 `1..10` / `1..=10`**・**ガード `if cond`**・ワイルドカード `_`・enum バリアント束縛・enum タグ比較による if-else 連鎖は実装済み。**残り**: 範囲の浮動小数境界の網羅性、識別子束縛パターン（`x ->` で scrutinee 全体を束縛）、ネストパターン、`|`（or パターン）、exhaustiveness 検査
- ラムダ `(x): T -> expr`、関数型シグネチャ
- 一般の型合成 `A + B`（ADR-0001。引数位置の匿名トレイト境界としては実装済み）
- ジェネリック **enum**（`Option`/`Result`/ユーザ定義）: スカラ／参照／`string`＝ptr ペイロードは i64 共通レイアウト、**struct 等の集約ペイロードは per-instantiation レイアウトで実装済み**（ADR-0010、`Option<Point>`・`Wrap<Point>` 等が縦断・clang 実行）。**残り**: 複数異種ペイロードの過小整列（記憶域＝最大サイズ型のため最大整列とは限らない）、ジェネリック関数の単相化の内側でのみ現れる集約インスタンスの型宣言収集（ADR-0010「Consequences」参照）
- ジェネリックな **struct 型**定義のコード生成（`type Pair<T>`/`type Box<T>` の per-instantiation 単相化）は**実装済み**（enum・関数のジェネリクスと合わせ、`tests/codegen.rs` で縦断・clang 実行）。**残り**: 整数/小数リテラル構築での非既定幅の型引数の文脈伝播、ジェネリック関数の内側でのみ現れるインスタンスの型宣言収集（ADR-0010 と同種）
- **`use`・モジュール解決（実装済み）**: `use a.b.*`（glob）・`use a.b { x, y }`（選択）・`use a.b`（Plain）+ `a.b.x(...)` モジュールパス呼び出し。ドット区切り＝ファイルパス区切り（`use foo.bar` → `foo/bar.iris`）。`std` は `CARGO_MANIFEST_DIR/std/` または実行ファイル隣から解決。pub 可視性（モジュールから pub アイテムのみ提供）。**残り**: `use` の選択インポートによる名前制限（現状 Named は Glob と同じ動作）、モジュール自身の相互 use、可視性のモジュール間強制（main 側 pub/private の制限）。
- **文字列操作（実装済み）**: 長さ `s.len()`（libc `strlen`）・添字 `s[i]`（i 番目のバイト→`u8`）・連結 `a.concat(b)`（`malloc`+`strcpy`+`strcat`、ヒープ結果はリーク許容）を `impl string`＋libc extern で縦断実装。**汎用添字 `expr[i]`** は固定長配列 `T[]`・`Vec<T>` にも対応（要素型を返す）。**残り**: 文字列補間・スライス・`push`/再代入索引・非 Copy 要素の索引
- 文字列補間（バッククォート `` `...{expr}...` ``）
- **`as` 型変換（実装済み）**: `expr as Type` を字句解析（`as` キーワード）〜構文解析（二項より強く・単項/後置より弱い優先順位、左結合で連鎖可）〜型検査（数値↔数値・`bool`→数値のみ許可。それ以外は拒否）〜所有権（被変換値の読み。参照は auto-deref）〜codegen（`trunc`/`sext`/`zext`／`sitofp`/`uitofp`／`fptosi`/`fptoui`／`fptrunc`/`fpext`。LLVM 表現が同一なら無変換）まで縦断実装。これにより `s[i] as i32`（`u8`→`i32`）等が書けるようになった。**残り**: ポインタ・参照・`string`・enum/struct との変換、別名（`type Meters = f64`）への変換、リテラルの後方確定との連携
- ブロックコメント

### バックエンド・解析（未着手）

- 借用検査の高度化（NLL 風の精密なライフタイム領域推論・部分ムーブ・ループ）— `compiler.md` の Open Question 領域
- 型推論の高度化（リテラルの後方からの確定、ジェネリクスの単一化）
- コード生成の拡張（文字列のスライス・補間、I/O 拡充）、WASM ターゲット、JIT（LLVM ORC/MCJIT — 要 LLVM 導入）
- 並行処理 — `concurrency.md` 未設計

## 仕様未確定のため独自に決めた点（要確認）

実装を進めるための暫定判断。本実装前にユーザー確認のうえ `docs/spec` を更新する。

- **再代入の `mut` 位置**: `let mut x = ...`（Rust風）と仮定。docs は「mut は型修飾子」とも書くため `let x: mut T` の可能性もある。
- **代入文**: `target = value` を文として追加（docs に文法記述がなかった）。
- **`else`**: `}` と同じ行に必要（改行をまたぐ `else` は未対応）。
- **範囲パターンの包含性**: docs は `1..10` の表記のみで上限の包含/排他を規定していないため、Rust に倣い `..` を排他（`lo <= x < hi`）・`..=` を包含（`lo <= x <= hi`）と仮定。
- **match のガード構文**: `pattern if cond -> body`（Rust 風）と仮定。docs に文法記述がなかった。
- **識別子束縛パターン未対応**: `match x { name -> ... }` の `name` は現状バリアント名として扱う（scrutinee 全体を束縛する識別子パターンは未実装）。そのため scrutinee 値を参照するガードは `_ if cond`（scrutinee を変数で持つ場合）の形で書く。
- **`for` の範囲構文**: spec（`control.md`）は `for x in list`（イテレータ）のみ規定。当面は **整数範囲 `for x in lo..hi` / `lo..=hi`** に限定して実装した。境界はリテラルに限らず**任意の整数式**を許す（`for i in 0..n`）。上限の包含/排他は範囲パターンと同じ規約（`..` 排他・`..=` 包含、Rust 準拠の暫定）。ループ変数 `x` は**不変束縛**（反復ごとに再束縛・Copy）。`for` キーワードは `impl Trait for Type` の `for` と同一トークン（文脈で判別）。
- **文字列操作の構文**: spec（`type.md`）は補間のみ規定し、長さ・索引・連結の構文は未定。`+` は型合成（ADR-0001）のため連結に使えないので、`s.len()` / `a.concat(b)`（`impl string` のメソッド）・添字 `s[i]`（→`u8`）を暫定採用した。`impl` 対象に組み込みプリミティブ（`string`）を許す点も暫定（メソッド表は型名で引くため動作する）。
- **`string` は Copy**: 不変な NUL 終端ポインタで free/drop を持たない（concat の結果はリーク）ため Copy 型として扱う（ムーブしない）。Rust の `&str` 相当。spec に所有権上の規定は無いため暫定。将来 free/所有を導入する場合は再検討が必要。
- **添字 `s[i]` はバイト**: 文字列の索引は UTF-8 バイト列の i バイト目を `u8` で返す（`char` リテラル・codegen 未整備のため）。マルチバイト境界・`char` 単位の索引は未対応。`u8`→`i32` の暗黙幅変換は無いが、`as` 型変換が実装されたため `println_int(s[i] as i32)` のように明示変換して書ける。
- **`as` 型変換の構文・範囲**: spec に型変換の記述が無いため Rust に倣い `expr as Type` を採用。`as` を予約語化し、優先順位は二項演算子より強く・単項/後置より弱い（`a + b as T` = `a + (b as T)`、`-x as T` = `(-x) as T`）。左結合で `x as A as B` も可。変換は当面 **数値↔数値・`bool`→数値**に限定（ポインタ・参照・`string`・enum/struct・別名への変換は未対応）。`bool`→整数は符号なし拡張（`zext`、0/1）。浮動小数→整数はゼロ方向丸め（`fptosi`/`fptoui`）。
- **NULL 判定は述語 `is_null` に集約**: ポインタ NULL の検出に `null` リテラル＋ポインタ比較ではなく、組み込み述語 `is_null(p): bool` を採用した（spec に NULL 規定が無いため暫定）。生ポインタを言語表層へ出さない self-contained 志向に沿い、FFI の NULL は std ラッパ（`open`/`env`）が `Option` に変換して隠す。`null` リテラル・ポインタ算術・任意ポインタ比較は導入していない。
- **`malloc` の引数幅**: prelude では `extern fn malloc(n: i32)` と宣言し、`Vec` リテラルの確保も i32 引数で呼ぶ（`as` 変換が無く concat の長さが i32 のため）。x86-64 では i32 引数が rdi へゼロ拡張されるため libc の `size_t`（i64）と ABI 互換。他ターゲットへ移す際は要再検討。
- **Drop/free の第一スライス範囲**: 解放対象は **`Vec<T>` ローカルのみ**（`string` は ADR で Copy・leak 許容、`Box` は codegen 未整備のため対象外）。条件付き move は **動的 drop flag**（Rust 準拠）で解決し、**解放位置はスコープ末＝関数末**（最後の使用での即時解放や、ループ本体・ネストブロック単位の早期解放は未実装）。move 検出は codegen が「裸の Vec 識別子の値消費」を消費地点ごとに記録する方式（所有権チェッカ flow.rs は不変＝use-after-move は従来どおり静的に拒否し、drop flag は解放責務の追跡のみ）。`free` は libc を直接 `declare`（prelude には出さない）。健全（二重解放・use-after-free 無し）だが、ループ内 Vec・Vec 引数・struct フィールドの Vec は現状リークを許容する。

## ビルド・実行

```sh
cargo build
cargo test
cargo run -- examples/hello.iris              # AST を表示

# 実行ファイルを生成・実行（i32/bool の部分集合のみ）
cargo run -- run examples/print.iris          # 生成して即実行（既定 -O0）
cargo run -- build examples/print.iris        # 実行ファイルを生成（既定 -O0=高速）
cargo run -- build --release -o prog examples/print.iris   # -O2（最適化）
cargo run -- --emit-llvm examples/print.iris  # LLVM IR を表示
```

ビルドは既定 **-O0**（開発用・高速）、`--release` で **-O2**。リンク/実行ファイル化は
`clang` に委ねる。計測例（800関数）: `-O0` 156ms vs `-O2` 1468ms（約9倍差）。最適化の
重さがビルド時間を支配するため、開発ループは -O0 既定で速い。

### 速度検証（ベンチ）

再現可能なベンチを `examples/` に用意（リリースで実行）:

```sh
cargo run --release --example bench_compile     # コンパイル速度（フロント throughput と -O0/-O1/-O2）
cargo run --release --example bench_ownership   # 所有権 DAG（型グラフ循環検出・借用グラフ）の速度
```

計測の要点:
- **フロントエンド＋コード生成**（`compile_ir`、clang 除く）は概ね線形で **~20〜40万行/秒**（in-process 計測）。
- **ビルド総時間は clang の最適化レベルが支配**: 1000関数で -O0 ≈145ms / -O1 ≈1736ms / -O2 ≈1643ms（-O0→-O1 で約11倍）。→ 開発は -O0 既定が効く。
- 所有権解析はマイクロ〜ミリ秒オーダーで、ボトルネックにならない（借用競合チェックは生存中の借用を場所ごとに索引し、借用数に対して線形 O(M)）。
