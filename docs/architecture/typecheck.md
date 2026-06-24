# 型検査：呼び出しグラフと関数仕様

エントリポイントは `sema::check()`（`src/sema/typeck.rs:137`）。

## コールグラフ

```mermaid
flowchart TD
    check["check()\nProgram全体を走査し各Itemを振り分ける"]
    check_function["check_function()\n関数1件の型パラメータ・引数型・戻り値型を設定し本体を検査"]
    check_block["check_block()\nBlock内の全Stmtを順に検査"]
    check_stmt["check_stmt()\nLet/Return/Assign/Expr/While/For等の文を検査"]
    check_expr["check_expr()\n式を検査してTyを返す（infer_exprに委譲）"]
    infer_expr["infer_expr()\n式の形に応じて具体的な推論ルーティング"]
    infer_call["infer_call()\n関数呼び出し・組み込みの引数型と戻り値型を推論"]
    check_func_call["check_func_call()\n名前付き関数の引数個数・型を検査"]
    infer_method_call["infer_method_call()\nメソッド呼び出しの提供元解決と型推論"]
    infer_binary["infer_binary()\n二項演算の被演算子型を検査し結果型を返す"]
    infer_unary["infer_unary()\n単項演算の型検査"]
    infer_member["infer_member()\nフィールドアクセスの型解決"]
    infer_struct_lit["infer_struct_lit()\n構造体リテラルのフィールド型整合性検査"]
    infer_match["infer_match()\nmatch腕の型統一"]
    infer_cast["infer_cast()\nas キャストの有効性検査"]
    check_conformance["check_conformance()\nimpl Trait for Type の完全性検査"]
    validate_type["validate_type()\n型名が既知かを検証"]

    check --> check_function
    check --> check_conformance
    check_function --> validate_type
    check_function --> check_block
    check_block --> check_stmt
    check_stmt --> check_expr
    check_expr --> infer_expr
    infer_expr --> infer_call
    infer_expr --> infer_binary
    infer_expr --> infer_unary
    infer_expr --> infer_member
    infer_expr --> infer_struct_lit
    infer_expr --> infer_match
    infer_expr --> infer_cast
    infer_call --> check_func_call
    infer_call --> infer_method_call
```

## 関数仕様表

| 関数 | 入力 | 出力 | 副作用 |
|---|---|---|---|
| `check()` | `&Program`, `&Resolution` | `Result<TypeInfo, Vec<TypeError>>` | `Checker` を構築し全 Item を走査 |
| `check_function()` | `&Function` | なし | `self.generics` に型パラメータを設定、`check_block()` を呼ぶ |
| `check_block()` | `&Block` | なし | 各 Stmt に `check_stmt()` を呼ぶ |
| `check_stmt()` | `&Stmt` | なし | Let/Return/Assign の型整合を検査、`check_expr()` を呼ぶ |
| `check_expr()` | `&Expr` | `Ty` | `infer_expr()` に委譲し `expr_types` に記録 |
| `infer_expr()` | `&Expr` | `Ty` | ExprKind で分岐して専用推論関数へルーティング |
| `infer_call()` | callee `&Expr`, args `&[Expr]`, span | `Ty` | ジェネリック単相化情報を `mono` に記録 |
| `check_func_call()` | name, span, args, arg_tys | `Ty` | 引数個数・型不一致をエラーに追加 |
| `infer_method_call()` | object, method, args, span | `Ty` | `method_provider` に提供元ラベルを記録 |
| `infer_binary()` | op, lhs, rhs | `Ty` | 被演算子の型不一致をエラーに追加 |
| `infer_member()` | object, field, span | `Ty` | struct のフィールド型を解決 |
| `infer_struct_lit()` | name, fields, span | `Ty` | フィールド名・型の整合性を検査 |
| `infer_match()` | scrutinee, arms, span | `Ty` | 腕パターン型とスクルティニー型を照合、腕ボディの型を統一 |
| `check_conformance()` | なし | なし | `impl Trait for Type` の全メソッドが揃っているかを検査 |
| `validate_type()` | `&Type` | なし | 型名が `known_types` に存在するかを検証、エラーを追加 |

## 重要な状態（`Checker` 構造体）

| フィールド | 役割 |
|---|---|
| `expr_types: HashMap<Span, Ty>` | 各式の型（→ `TypeInfo` へ移す） |
| `mono: HashMap<Span, (String, Vec<Ty>)>` | ジェネリック呼び出しの単相化情報 |
| `def_types: Vec<Ty>` | DefId ごとの型（引数・ローカル変数） |
| `generics: HashMap<String, Vec<Bound>>` | 現在の関数の型パラメータ→境界 |
| `errors: Vec<TypeError>` | 収集したエラー（複数件まとめて報告） |
