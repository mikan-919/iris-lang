# ADR-0002: エラー伝播演算子は `!`、`?` は三項演算子

## Status
Accepted

## Context
Rustの `?` 演算子はエラー早期リターンに使われるが、irisでは `?` を三項演算子（`cond ? a : b`）に割り当てたい。

## Decision
- `!` をエラー早期リターン演算子とする: `result!`
- `?` は三項演算子専用: `cond ? a : b`

## Consequences
Rustユーザーには `!` の意味が異なるため注意が必要。SwiftのforceUnwrap（`!`）とも異なり、irisの `!` はpanicではなく伝播。
