# データフロー：主要データ構造の追跡

各段で受け渡されるデータ構造を追跡する。推測なし・ソースコードから確認済み。

## フロー概要

```
&str  (プレリュード前置済みソース)
  ↓ lexer::lex()
Vec<Token>
  ↓ parser::parse()
Program
  ↓ sema::resolve()
Resolution
  ↓ sema::check()
TypeInfo
  ↓ sema::check_ownership()
() [検査のみ、新たな値なし]
  ↓ codegen::emit_module()
String (LLVM IR テキスト)
  ↓ clang
実行ファイル
```

---

## `Vec<Token>`

| 項目 | 内容 |
|---|---|
| 定義場所 | `src/token.rs` の `Token` struct |
| 主なフィールド | `kind: TokenKind`（識別子・整数・演算子・改行・Eof 等）、`span: Span`（オフセット＋長さ） |
| 作成者 | `lexer::lex()` |
| 利用者 | `parser::parse()` |

`Span` は `src/span.rs` の `Span { offset: usize, len: usize }`。
エラー表示・型情報・名前解決でノードの同一性を区別するキーとして多用される。

---

## `Program`

| 項目 | 内容 |
|---|---|
| 定義場所 | `src/ast.rs` |
| 主なフィールド | `items: Vec<Item>` |
| 作成者 | `parser::parse()` |
| 利用者 | `resolve()`、`check()`、`check_ownership()`、`emit_module()` 全段で参照 |

`Item` は `Function` / `TypeDef` / `Trait` / `Impl` / `Use` のいずれか。
`Function` は `name`・`params`・`ret`・`body: Block`（`Vec<Stmt>` を包む）を持つ。
`Stmt` は `Let` / `Return` / `Assign` / `Expr` / `While` / `For` 等。
`Expr` は `kind: ExprKind` と `span: Span` を持つ。`ExprKind` は `Int`・`Bool`・`Str`・
`Ident`・`Call`・`Binary`・`If`・`StructLit` 等。

---

## `Resolution`

| 項目 | 内容 |
|---|---|
| 定義場所 | `src/sema/resolve.rs` |
| 主なフィールド | `defs: Vec<Def>`（全定義）、`uses: HashMap<Span, DefId>`（使用位置→定義ID）、`module_fn_calls: HashMap<Span, String>` |
| 作成者 | `sema::resolve()` |
| 利用者 | `check()`、`check_ownership()`、`emit_module()` |

`Def` は `kind: DefKind`（Function / Param / Local / Builtin）・`name`・`span`・`mutable`。
`DefId` は `usize`（`defs` のインデックス）。識別子の使用位置 span をキーとして、
その定義 DefId を引ける。

---

## `TypeInfo`

| 項目 | 内容 |
|---|---|
| 定義場所 | `src/sema/typeck.rs` |
| 主なフィールド | `expr_types: HashMap<Span, Ty>`（式span→型）、`mono: HashMap<Span, (String, Vec<Ty>)>`（単相化情報）、`method_provider`、`variant_constructions`、`for_iter_elem` |
| 作成者 | `sema::check()` |
| 利用者 | `check_ownership()`、`emit_module()` |

`Ty` は `src/sema/ty.rs` の内部型表現。`Named { name, args }`・`Ref { mutable, inner }`・
`Array`・`Tuple`・`IntLit`（未確定）・`FloatLit`（未確定）・`Infer`・`Error` のいずれか。
`emit_module()` は `expr_types` を見て各式の LLVM 型を決定し、`mono` を見てジェネリック
関数の単相化記号（`@fn.i32` など）を作る。

---

## `String`（LLVM IR テキスト）

| 項目 | 内容 |
|---|---|
| 形式 | LLVM IR テキスト（`.ll` 形式）。`define`・`declare`・`%struct`・`@.str.N` を含む |
| 作成者 | `codegen::emit_module()` |
| 利用者 | `main.rs::build()` が一時ファイルに書き出し `clang` に渡す |

`emit_module()` が内部で `String` バッファを直接構築する（`fmt::Write` を使用）。
`FnCodegen` 構造体が関数ごとの IR バッファを持ち、完成後に module バッファへ連結する。
