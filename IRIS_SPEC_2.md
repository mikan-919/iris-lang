# Iris Language 仕様書 Draft v0

## 1. 概要

Iris は「主体(Subject)」を中心とした flow-based 汎用プログラミング言語である。

この言語の中心概念は値ではなく主体である。

```iris
input
:: trim()
:: validate()
:: save()
```

各ステップは現在主体を受け取り、新しい主体を返す。

メソッド呼び出しではなく、

```iris
trim(input)
```

に近い意味を持つ。

---

# 2. 主体フロー

## transform

```iris
value
:: foo()
:: bar()
```

は概念的に

```iris
bar(
    foo(
        value
    )
)
```

である。

---

## 第一引数主体

関数は原則として第一引数が主体である。

```iris
text
:: trim()
```

↓

```iris
trim(text)
```

---

## 主体位置指定 `$`

主体を第一引数以外へ渡す。

```iris
text
:: includes(baseText, $)
```

↓

```iris
includes(baseText, text)
```

---

## 主体エイリアス `$name`

主体に一時名を与える。

```iris
foo$x(
    x + bar(x)
)
```

---

## Headless Pipeline

主体を持たないパイプラインを生成できる。

```iris
(
    :: trim()
    :: lower()
)
```

これは値ではなく Flow オブジェクトである。

---

# 3. 分岐

## if

```iris
value
:if condition()
:then foo()
:else bar()
```

---

## Predicate Pipeline

```iris
value
:if(
    :: trim()
    :: isEmpty()
)
:then handleEmpty()
:else process()
```

---

## Expression Branch

```iris
value
:else$x x + 10
```

---

# 4. ループ

## while

```iris
stream
:while hasNext()
:then process()
```

---

# 5. 副作用分岐 `%`

親主体を置換しない。

```iris
value
:then% log()
```

意味:

```iris
log(value)
return value
```

---

# 6. エラー処理

```iris
value
:: parse()
:catch$error recover(error)
```

`$error` はエラー主体。

---

# 7. 非同期

## Promise

```iris
Promise<T>
```

は未来の主体。

---

## await

```iris
promise
:await
```

↓

```iris
T
```

---

# 8. Stream

Stream は複数主体を未来に供給する。

```iris
stream
:: pull()
```

は概念的に

```iris
Promise<Option<T>>
```

に近い。

---

# 9. 型システム

## 型

```iris
String
Number
File
```

など。

具体的な実体。

---

## Trait

```iris
#Text
#String
#Readable
```

など。

Trait は能力宣言である。

---

## 型と Trait

```iris
String
```

と

```iris
#String
```

は別概念。

```txt
String ∈ #String
```

という所属関係を持つ。

---

# 10. Trait 継承

```iris
trait #String : #Text
```

意味:

```txt
#String
=
#Text
+
追加能力
```

親 Trait の能力は全て継承される。

---

# 11. Trait 定義

```iris
trait #Text {
    trim()
    len()
}
```

Trait は能力契約を定義する。

---

## デフォルト実装

```iris
trait #Text {
    trim() {
        ...
    }
}
```

を許可する。

---

# 12. Impl

```iris
impl #String for String {
    trim(...) {
        ...
    }
}
```

型向け Trait 実装を提供する。

---

## 契約充足

```iris
trait #Text {
    trim()
    len()
}
```

に対し

```iris
impl #String for String {
    trim()
}
```

しか無い場合、

`len` が親 Trait のデフォルト実装で補完できなければエラー。

---

# 13. Override Chain

Override は継承系列のみ許可。

```txt
trait contract
↓
parent trait default
↓
child trait default
↓
concrete type implementation
```

例:

```txt
#Text.trim
↓
#String.trim
↓
String.trim
```

---

# 14. Module

関数はモジュールに属する。

```iris
buffer.convert()
bigint.convert()
```

---

## use

```iris
use std::*
```

は

```txt
名前導入
+
実装探索空間への参加
```

を意味する。

---

# 15. 関数探索

探索順序:

```txt
主体型
↓
use済みモジュール
↓
関数名
↓
引数条件
```

---

## 解決規則

```txt
候補0件
→ 解決失敗

候補1件
→ 採用

候補2件以上
→ 曖昧性エラー
```

---

## 重要原則

コンパイラは推測しない。

以下は不採用。

```txt
より具体的
より近い
より適切そう
```

---

# 16. Specialization

一般 Specialization は存在しない。

```iris
convert(#Number)
convert(Int32)
```

が同時に見える場合、

```txt
曖昧性エラー
```

となる。

---

# 17. Override による隠蔽

唯一の例外。

```iris
trait #Text {
    trim()
}

trait #String : #Text {
    trim()
}
```

の場合、

```txt
#String.trim
```

が

```txt
#Text.trim
```

を隠蔽する。

---

# 18. Trait 系列指定

競合する実装群が存在する場合、

利用者が系列を指定する。

```iris
text
:: std.#String.trim()
```

意味:

```txt
std.#String 系列
↓
trim探索
↓
override解決
```

---

# 19. require

require は comptime 制約。

```iris
!require(#Readable)
type File = ...
```

---

## 意味

```txt
File ∈ #Readable
```

を要求する。

---

## 特徴

require は能力要求であり証明規則ではない。

```txt
require
=
何を要求するか

impl
=
どう証明するか
```

---

## 複数 require

```iris
!require(#Readable)
!require(#Writable)

type File = ...
```

意味:

```txt
File ∈ #Readable
AND
File ∈ #Writable
```

---

## 伝播しない

```iris
!require(#Serializable)
type Box<T> = ...
```

から

```txt
T ∈ #Serializable
```

は導かれない。

---

## 検査

コンパイル到達時に検査される。

未到達型は検査対象にならない。

---

# 20. comptime directive

`!` は comptime directive。

```iris
!require(...)
type File = ...
```

は

```iris
comptime {
    require(...)
}
```

に近い概念である。

将来的に

```iris
!derive(...)
!inline
!ffi(...)
```

なども同系統へ統合される。

---

# 21. ジェネリクス

能力制約はジェネリクスで表現する方向。

```iris
trim<T:#Text>(
    text: T
) -> T
```

---

能力合成:

```iris
save<T:#Text && #Serializable>(...)
```

---

# 22. 現在の根本原則

1. 主体が言語の中心
2. メソッドではなく関数探索
3. Trait は能力宣言
4. struct + trait を基本構成とする
5. override chain 以外の競合は全てエラー
6. コンパイラは意図を推測しない
7. require は要求であり証明ではない
8. 実装探索範囲は use が決定する
9. Trait 継承のみが優先順位を持つ
10. Flow を型システムより上位概念として扱う
