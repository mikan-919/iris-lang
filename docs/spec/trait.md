# Trait（トレイト）

トレイトは「型に**振る舞い**を付け足す」仕組み。言語の核心の一つ。
「この型はこういうことができる」という約束をまとめたもので、
継承ではなく**振る舞いの共有**で型をつなぐ。

## 定義：できることの一覧を宣言する

```iris
trait Greet {
    fn hello(self): string
}
```

これは「`Greet` を名乗る型は `hello` を持つこと」という約束だけを書いた状態。
中身（実装）は各型が後で埋める。

`self` は「その値自身」を指す第一引数で、**明示する**（暗黙の `this` は無い）。

### デフォルト実装

約束のところに最初から中身を書いておける。各型はそのまま使ってもよいし、
`impl` 側で**上書き（オーバーライド）**してもよい。

```iris
trait Greet {
    fn hello(self): string {
        return "Hello"
    }
}
```

## 実装：型に振る舞いを与える

`impl ... for ...` で「この型はこのトレイトを満たす」と宣言し、中身を書く。

```iris
impl Greet for Name {
    fn hello(self): string {
        return "Hello"
    }
}
```

## 型固有のメソッド

トレイトを介さず、型に直接メソッドを生やすこともできる。
その型だけの便利関数を置く場所。

```iris
impl Point {
    fn new(x: f64, y: f64): Point { ... }   // コンストラクタ的な関数
    fn distance(self): f64 { ... }
}
```

## 呼び出し

ドット記法で呼ぶ。

```iris
let name = Name { ... }
name.hello()
```
