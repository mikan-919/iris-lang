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
- 反復可能な標準コレクション（`Vec<T>` 等）はジェネリック struct の単相化が前提のため未提供。

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
