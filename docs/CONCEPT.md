---
name: iris-lang
---
# iris-lang — Concept

## Single job
GoのようなシンプルさでRustの所有権ベースのメモリ安全性を得られるコンパイル言語。

## Audience & state
自分自身のために作る。Rustのborrow checkerに疲れた、あるいはGoでもっと型安全に書きたいと感じているプログラマーにも広まってほしい。

## Values
- **明示性** — 暗黙のランタイムコスト（GC、ARC）を禁じる。
- **シンプルさ** — 所有権の複雑さを呼び出し側に見せない。ライブラリ作者側だけが意識する。
- **書き心地** — 型安全を犠牲にせずGoのように読める構文を保つ。

## Subject world
- トレイト（Rustに倣った、言語の核心）
- `mut` / `ref` / `nobind` 型修飾子（宣言側に集中）
- 所有権DAG（コンパイラが管理、呼び出し側は意識しない）
- `let` / `const`（再代入の意図は宣言側の`mut`で表現）
- enum / struct
- TypeScript風の関数定義: `fn add(a: number, b: number): number { ... }`

## Forbidden moves
- GC — 明示性に反する。
- ARC — 明示性に反する（暗黙のランタイストコスト）。
- `var` — スコープの曖昧さを生む。書き心地に反する。
- `;` — 不要なノイズ。書き心地に反する。
- 呼び出し側への所有権の露出 — シンプルさに反する。

## Content shape
改行がステートメントの区切り。型は宣言側に集中し、呼び出し側はクリーンに保つ。
