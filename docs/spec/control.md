# Control Flow

## 条件分岐

```
if condition {
    ...
} else if condition {
    ...
} else {
    ...
}
```

条件式を括弧で囲まない。

## 三項演算子

```
let x = condition ? a : b
```

## ループ

```
for x in list { ... }   // イテレータ
while condition { ... } // 条件ループ
loop { ... }            // 無限ループ
```

C式の `for (i = 0; i < n; i++)` は存在しない。

### 整数範囲の `for`（実装済み）

`for` の対象として **整数範囲** を扱える。

```
for i in 0..n { ... }    // 上限排他: 0, 1, ..., n-1
for i in 1..=n { ... }   // 上限包含: 1, 2, ..., n
```

- 上限の包含/排他は範囲パターン（`match`）と同じ規約: `..` は排他、`..=` は包含。
- 下限・上限は任意の整数式（リテラル・変数いずれも可）。両者は同じ整数型であること。
- ループ変数（`i`）は不変束縛で、反復ごとに 1 ずつ増える。`break` / `continue` を使える。

### 一般イテレータの `for`（実装済み・ADR-0007）

`for` の対象が範囲（`..` / `..=`）でないときは、`Iterator<T>` トレイトを実装する値の反復として扱う。

```
trait Iterator<T> { fn next(&mut self): Option<T> }   // std が提供

for x in iter { ... }    // iter: Iterator<T> を実装、x: T
```

- 意味論は `loop { match iter.next() { Some(x) -> { ... }, None -> break } }` 相当。
  毎反復で `iter.next()` を呼び、`Some(x)` なら要素 `x`（型 `T`）を束縛して本体を実行、`None` で終了。
- `iter` は反復のあいだ可変借用される（`next(&mut self)`）。ループ変数 `x` は反復ごとに再束縛。
- `Iterator` を実装しない型は拒否。同じ型が複数の `Iterator`（`Iterator<i32>` と `Iterator<string>` 等）を
  実装する場合の曖昧解消 `for x#T in iter`（ADR-0007）は未実装（当面は単一実装のみ）。

### 配列・動的配列の `for`（実装済み）

`for` の対象が固定長配列 `T[]` または動的配列 `Vec<T>` のとき、その要素を直接反復する
（`Iterator` 実装より優先する組み込み挙動）。

```
let a: i32[] = [1, 2, 3]
for x in a { ... }          // x: i32（各要素）

for y in [10, 20, 30] { ... }   // 配列リテラルを直接反復
```

- 意味論は `0..len` のインデックスループ。要素を順に取り出してループ変数へ束縛する。
- コレクションは**借用**され消費しない（反復後も再利用できる）。ループ変数 `x` は反復ごとに値で
  再束縛され、`break` / `continue` を使える。
- 当面、要素は **Copy 型のみ**（数値・`bool`・`char`・不変参照）。非 Copy 要素（struct・`string` 等）の
  反復は要素のムーブ/借用の設計が必要なため未対応。

## match

`match` は式として値を返せる。`if` も同様。

```
let result = match value {
    Ok(x) -> x
    Err(e) -> 0
}

let y = if condition { 1 } else { 0 }
```

対応するパターン：

```
match value {
    0 -> "zero"
    1..10 -> "small"
    Ok(x) -> x
    Some(x) if x > 0 -> "positive"
    _ -> "other"
}
```

- リテラル
- 範囲（`1..10`）
- enumのアンラップ（`Ok(x)`, `Some(x)`）
- ガード（`if`条件）
- ワイルドカード（`_`）
