# Function

## 定義

```
fn add(a: number, b: number): number {
    return a + b
}
```

戻り値型は `:` で後置。

## 関数型シグネチャ

```
type Func = (number, (number): number): void
```

## ラムダ

`->` はラムダであることを示す目印。式本体を導く。`fn` のブロック本体とは別物。

```
let f = (x: number): number -> x * 2
```

## ジェネリクス

```
fn map<T, U>(list: List<T>, f: (T): U): List<U> { ... }
```

## 三項演算子

```
let x = condition ? a : b
```
