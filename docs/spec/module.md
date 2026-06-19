# Module

## 可視性

デフォルトは非公開。`pub` で公開する。

```
pub fn foo() { ... }
pub type Bar = struct { ... }
pub trait Greet { ... }
```

## インポート

```
use std.lib
use std.lib { put, error }
use std.lib.*
```
