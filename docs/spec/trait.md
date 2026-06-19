# Trait

トレイトは型に振る舞いを付与する仕組み。言語の核心。

## 定義

```
trait Greet {
    fn hello(self): string
}
```

デフォルト実装を持たせることもできる。`impl`側でオーバーライド可能。

```
trait Greet {
    fn hello(self): string {
        return "Hello"
    }
}
```

- `self` は第一引数として明示する。

## 実装

```
impl Greet for Name {
    fn hello(self): string {
        return "Hello"
    }
}
```

## 型固有のメソッド

トレイトなしで型にメソッドを定義できる。

```
impl Point {
    fn new(x: f64, y: f64): Point { ... }
    fn distance(self): f64 { ... }
}
```

## 呼び出し

ドット記法を使う。

```
let name = Name { ... }
name.hello()
```
