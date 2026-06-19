# ADR-0001: 型合成演算子は `+`、参照は `&`

## Status
Accepted

## Context
型合成（トレイト境界・型の交差）に `&` を使うと、参照構文（`&Type`, `&mut Type`）と競合する。

## Decision
- `&` は参照専用: `&Type`（不変参照）、`&mut Type`（可変参照）
- `+` は型合成専用: `A + B`、`Greet + Serialize`

## Consequences
Rustのトレイト境界（`T: A + B`）と記法が近く、Rustユーザーには自然。TypeScriptの`&`による intersection typeとは異なる。
