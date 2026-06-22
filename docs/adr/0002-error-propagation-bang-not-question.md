# ADR-0002: エラー伝播演算子は `!`、`?` は三項演算子

## Status
Accepted・実装済み（`!` を構文解析〜型検査〜所有権〜codegen まで縦断。`expr!` は Result/Option を
評価し、成功ならアンラップ、失敗なら関数の戻り型へ `Err(e)`/`None` を再構築して早期 return。
`?` は三項演算子として実装済み。`tests/codegen.rs`・`tests/typeck.rs`）

## Context
Rustの `?` 演算子はエラー早期リターンに使われるが、irisでは `?` を三項演算子（`cond ? a : b`）に割り当てたい。

## Decision
- `!` をエラー早期リターン演算子とする: `result!`
- `?` は三項演算子専用: `cond ? a : b`

## Consequences
Rustユーザーには `!` の意味が異なるため注意が必要。SwiftのforceUnwrap（`!`）とも異なり、irisの `!` はpanicではなく伝播。
