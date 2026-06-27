# Import（インポート）

他の場所で定義された物を、今のファイルに持ち込む構文。
（可視性・公開側のルールは `module.md` 参照。）

## 構文：3通りの取り込み方

```iris
use std.lib              // モジュールとして取り込む
use std.lib { put, error }   // 指定した物だけ名前で取り込む
use std.lib.*            // 中身を全部取り込む
```

## アクセス

取り込んだ物はドット記法でたどる。

```iris
std.lib.fmt("Hello!")
```
