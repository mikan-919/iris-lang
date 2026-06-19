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
