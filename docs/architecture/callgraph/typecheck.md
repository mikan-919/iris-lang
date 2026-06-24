# 型検査コールグラフ

`check()` から実際に呼ばれる関数のみ。深さ 5 以下、20 ノード以下。

```mermaid
flowchart TD
    check["check()\nProgram全体のエントリポイント"]
    check_function["check_function()\n関数1件の検査"]
    check_block["check_block()\nブロック内の全Stmtを順に処理"]
    check_stmt["check_stmt()\n各文の型整合を検査"]
    check_expr["check_expr()\n式を検査しTyを返す"]
    infer_expr["infer_expr()\n式種別でルーティング"]
    infer_call["infer_call()\n関数呼び出しの型推論・単相化"]
    check_func_call["check_func_call()\n引数個数・型の照合"]
    infer_method_call["infer_method_call()\nメソッドの提供元解決と型推論"]
    infer_binary["infer_binary()\n二項演算の型検査"]
    infer_member["infer_member()\nフィールドアクセスの型解決"]
    infer_match["infer_match()\nmatch腕の型統一"]
    check_conformance["check_conformance()\nimpl Trait の完全性検査"]
    validate_type["validate_type()\n型名の有効性検証"]

    check --> check_function
    check --> check_conformance
    check_function --> validate_type
    check_function --> check_block
    check_block --> check_stmt
    check_stmt --> check_expr
    check_expr --> infer_expr
    infer_expr --> infer_call
    infer_expr --> infer_binary
    infer_expr --> infer_member
    infer_expr --> infer_match
    infer_call --> check_func_call
    infer_call --> infer_method_call
```

## 各ノードの責務

| ノード | 責務 |
|---|---|
| `check()` | Program の全 Item を走査し Function/Impl/Trait を振り分ける |
| `check_function()` | 型パラメータ環境を設定し、引数型を DefId に登録して本体を検査する |
| `check_block()` | Block 内の全 Stmt を順に `check_stmt()` に渡す |
| `check_stmt()` | Let/Return/Assign/Expr/While/For の型整合を検査する |
| `check_expr()` | `infer_expr()` に委譲し結果を `expr_types` に記録する |
| `infer_expr()` | ExprKind で分岐して専用推論関数へルーティングする |
| `infer_call()` | 引数の型推論・ジェネリック単相化情報を `mono` に記録する |
| `check_func_call()` | 引数個数・型の不一致をエラーに追加する |
| `infer_method_call()` | 提供元ラベルを解決し `method_provider` に記録する |
| `infer_binary()` | 被演算子の型要件を検査し結果型を返す |
| `infer_member()` | struct のフィールド名と型を解決する |
| `infer_match()` | scrutinee 型と腕パターンを照合し腕ボディの型を統一する |
| `check_conformance()` | 全 `impl Trait for Type` のメソッド実装の過不足を検査する |
| `validate_type()` | 型名が `known_types` に含まれるかを検証する |
