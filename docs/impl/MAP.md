# 実装対応マップ

仕様・トークン・AST・evalの対応を一覧にしたもの。「今どこを触るか」に迷ったらここを見る。

---

## バックボーン演算子

| 構文 | トークン (`token/token.go`) | AST node (`ast/ast.go`) | eval (`eval/eval.go`) |
|---|---|---|---|
| `=:` | `OP_INITIATE` | `LetStatement.Value` の先頭 | `evalLet` |
| `::` | `OP_NEXT` | `PipelineStep{Op: OP_NEXT}` | `evalTransformStep` |
| `:~` / `:await` | `OP_AWAIT` / `OP_AWAIT_KWORD` | `PipelineStep` | 現在は sync として扱う |
| `:^` | `OP_TRY` | `PipelineStep` | エラーなら即リターン |
| `:!` | `OP_FORCE` | `PipelineStep` | **廃止** → `@unwrap` に置き換え |
| `:?` | `OP_CATCH` | `PipelineStep` | エラーをデフォルト値で置換 |
| `:\|` | `OP_OR` | `PipelineStep` | falsyなら置換 |
| `:>` | `OP_TAG` | `PipelineStep` | タグ名でsnapshotを保存 |
| `:&` | `OP_JOIN` | `PipelineStep` | `TupleValue{subject, tag}` を返す |

---

## キーワード演算子

| 構文 | トークン | AST node | eval |
|---|---|---|---|
| `:if cond :then body :else fallback` | `OP_IF` / `OP_THEN` / `OP_ELSE` | `IfExpression` | `evalIfStep` |
| `:then%` (副作用) | `OP_THEN` + `SideEffect:true` | `IfExpression{SideEffect:true}` | bodyを実行しsubjectを返す |
| `:while cond :then body` | `OP_WHILE` / `OP_THEN` | `WhileExpression` | `evalWhileStep` |
| `:catch $alias body` | `OP_CATCH_NAMED` | `CatchNamedExpression` | `evalCatchNamed` |

---

## 式・リテラル

| 構文 | AST node | eval |
|---|---|---|
| `42` / `3.14` / `"str"` / `true` | `IntLiteral` / `FloatLiteral` / `StringLiteral` / `BoolLiteral` | `evalExpr` の各case |
| `[a, b, c]` | `ArrayLiteral` | `evalArray` |
| `foo(a, b)` | `CallExpression` | `evalCall` |
| `foo(baseText, $)` の `$` | `Argument{IsSubject:true}` | 呼び出し時に現在subjectを渡す |
| `$` / `$name` | `SubjectRef` | 現在subjectまたはタグを返す |
| `a + b` / `a == b` など | `BinaryExpression` | `evalBinary` |
| `:: * 2` (pipeline内演算) | `OperatorApplication` | subjectを左辺として計算 |
| `(:: trim() :: lower())` | `HeadlessPipeline` | `FlowValue` を返す |
| `value :: match ( pat :: steps \| _ :: steps )` | `MatchExpression` / `MatchArm{Steps}` | `evalMatch` + `applySteps` |

---

## 型・Trait

| 構文 | AST node | 備考 |
|---|---|---|
| `String` / `File` | `NamedType` | 具体型 |
| `#Text` / `#Readable` | `TraitRef` | 能力宣言 |
| `#Text && #Serializable` | `TraitBoundType` | 複合Trait境界 |
| `T: #Text` | `TypeParam` | ジェネリクス型パラメータ |
| `trait #Text { ... }` | `TraitDeclaration` | |
| `impl #Text for String { ... }` | `ImplDeclaration` | |

---

## 宣言・ディレクティブ

| 構文 | トークン | AST node | eval |
|---|---|---|---|
| `let x =: ...` | `KW_LET` | `LetStatement` | `evalLet` |
| `fn foo(...) { }` | `KW_FN` | `FnDeclaration` + `BlockBody` | `FnValue` としてenvに登録 |
| `fn foo(...) =: ...` | `KW_FN` | `FnDeclaration` + `ExprBody` | 同上 |
| `export fn foo` | `KW_EXPORT` | `FnDeclaration{Exported:true}` | |
| `use std::*` | `KW_USE` | `UseStatement` | |
| `!require(#Readable)` | `BANG` | `ComptimeDirective` | comptime検査（未実装） |

---

## ランタイム値 (eval/value.go)

| 型 | 説明 |
|---|---|
| `IntValue` | int64 |
| `FloatValue` | float64 |
| `StringValue` | string |
| `BoolValue` | bool |
| `ArrayValue` | `[]Value` |
| `TupleValue` | `:&` joinの結果 `(a, b)` |
| `NilValue` | unit / 空 |
| `ErrorValue` | ランタイムエラー。`:?` / `:catch` で捕捉 |
| `FnValue` | ユーザー定義関数クロージャ |
| `FlowValue` | Headless Pipeline（呼び出し可能なFlow） |

---

## 未実装・TODO

| 機能 | 状態 |
|---|---|
| `:await` / `:~` | 構文はある、evalはsyncで代替 |
| `!require` comptime検査 | パースのみ、検査なし |
| `@unwrap` モディファイア | 仕様のみ、eval未実装 |
| Trait系列指定 `std.#String.trim()` | 未実装 |
| 関数探索（use済みモジュール経由） | 未実装 |
| 型チェッカー（本格実装） | スタブ状態 |
