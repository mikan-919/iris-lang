# Type

型は常にその値の参照の仕方を表す。

## 型エイリアス

`type` で定義した型は名前的型付けにより別の型として扱われる。

```
type Meters = f64
type Seconds = f64
// MetersをSecondsに渡すとコンパイルエラー
```

## 型変換

`as` で明示的に変換する。

```
let m: Meters = 5.0
let f: f64 = m as f64
```

## タプル

```
let t: (i32, string) = (42, "hello")
```

## コレクション

```
let arr: i32[] = [1, 2, 3]     // 固定長配列
let vec: Vec<i32> = [1, 2, 3]  // 動的配列
```

## 文字列補間

バッククォートで文字列補間ができる。

```
let name = "world"
let msg = `Hello, {name}!`
```

## null

null は存在しない。値の不在は `Option<T>` で表現する。

```
let x: Option<i32> = Some(42)
let y: Option<i32> = None
```

## 組み込みプリミティブ型

| 型 | 説明 |
|----|------|
| `i8`, `i16`, `i32`, `i64` | 符号付き整数 |
| `u8`, `u16`, `u32`, `u64` | 符号なし整数 |
| `f32`, `f64` | 浮動小数点数 |
| `bool` | 真偽値 |
| `string` | 文字列 |
| `char` | 文字 |

## 基本形

```
type Name = string
type Name = enum { Str: string, Num: number }
type Name = struct { key: i32 }
```

## 型修飾子

| 修飾子 | 意味 |
|--------|------|
| `&Type` | 不変参照 |
| `&mut Type` | 可変参照 |

## 型合成

`+` で複数の型・トレイトを合成する。

```
type Name = A + B
```

## トレイト境界

関数引数でも `+` を使う。

```
fn foo(x: Greet + Serialize): string { ... }
```

## ジェネリクス

`<T>` で型パラメータを表現する。

```
type List<T> = struct { ... }
fn map<T, U>(list: List<T>, f: (T): U): List<U> { ... }
```

型パラメータにトレイト境界を付ける場合は `:` を使う。

```
fn foo<T: Greet + Serialize>(x: T): string { ... }
```
