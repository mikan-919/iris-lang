# Error Handling

## Result型

エラーは `Result<T, E>` で表現する。

```
fn read(path: string): Result<string, Error> {
    let content = fs.read(path)!
    return Ok(content)
}
```

## `!` 演算子

`!` は Result/Option の早期リターン演算子。失敗時は呼び出し元に `Err` を伝播する。

```
let value = someResult!
```

`?` は三項演算子として予約されているため、早期リターンには `!` を使う。
