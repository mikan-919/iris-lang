# Hello World の経路追跡

```iris
fn main() {
    puts("hello")
}
```

このプログラムが実行ファイルになるまでに通過する関数と生成されるデータを段ごとに示す。

---

## 1. ソース入力（`main.rs::build()`）

| 項目 | 内容 |
|---|---|
| 入力データ | ファイルパス `hello.iris` |
| 出力データ | `src: String`（ファイル内容）|
| 通過関数 | `std::fs::read_to_string()` |
| 生成されるもの | `"fn main() {\n    puts(\"hello\")\n}"` |

---

## 2. プレリュード前置（`lib.rs::analyze()`）

| 項目 | 内容 |
|---|---|
| 入力データ | `src: &str` |
| 出力データ | `combined: String` |
| 通過関数 | `analyze()` 先頭で `format!("{PRELUDE}\n{src}")` |
| 生成されるもの | `extern fn puts(s: string): i32\n…fn main() { puts("hello") }` |

`PRELUDE` は `std/prelude.iris` のインライン文字列（`include_str!` で埋め込み）。

---

## 3. 字句解析（`lexer::lex()`）

| 項目 | 内容 |
|---|---|
| 入力データ | `combined_src: &str` |
| 出力データ | `Vec<Token>` |
| 通過関数 | `lex()` → `skip_trivia()` → `next_token()` の繰り返し |
| 生成されるもの | `[Ident("extern"), Kw("fn"), Ident("puts"), …, Ident("main"), LBrace, Ident("puts"), LParen, Str("hello"), RParen, Newline, RBrace, Eof]` |

`"hello"` は `TokenKind::Str` として span 付きでトークン化される。

---

## 4. 構文解析（`parser::parse()`）

| 項目 | 内容 |
|---|---|
| 入力データ | `&[Token]` |
| 出力データ | `Program { items: Vec<Item> }` |
| 通過関数 | `parse()` → `parse_item()` × N → `parse_function()` → `parse_block()` → `parse_stmt()` → `parse_expr()` |
| 生成されるもの | `Program { items: [Function(extern puts), …, Function(main)] }` |

`puts("hello")` は `Stmt::Expr(Expr { kind: ExprKind::Call { callee: Ident("puts"), args: [Str("hello")] } })` になる。

---

## 5. モジュール読み込み（`lib.rs::analyze()`）

`use` 宣言がないため `ModuleLoader` は何もしない。`prepend_items` は空。

---

## 6. 名前解決（`sema::resolve()`）

| 項目 | 内容 |
|---|---|
| 入力データ | `&Program`, `module_namespaces: HashMap` |
| 出力データ | `Resolution` |
| 通過関数 | `resolve()` → `Resolver` がトップレベル関数名を収集 → `main` の本体を走査 |
| 生成されるもの | `Resolution { defs: [Def { name:"puts", kind:Function }, Def { name:"main", kind:Function }], uses: { span_of("puts" in call) → DefId(0) } }` |

`puts` の callee span が `uses` で `DefId(0)` （puts の定義）に結びつく。

---

## 7. 型検査（`sema::check()`）

| 項目 | 内容 |
|---|---|
| 入力データ | `&Program`, `&Resolution` |
| 出力データ | `TypeInfo` |
| 通過関数 | `check()` → `check_function(&main)` → `check_block()` → `check_stmt()` → `check_expr()` → `infer_expr()` → `infer_call()` |
| 生成されるもの | `TypeInfo { expr_types: { span_of_call → Ty::Named("i32"), span_of_str → Ty::Named("string"), … } }` |

`puts` は `extern fn puts(s: string): i32` なので、
`infer_call()` が `check_func_call("puts", …)` を呼び引数 `string` を確認。
戻り値型 `i32` を call 式の span に記録する。

---

## 8. 所有権検査（`sema::check_ownership()`）

| 項目 | 内容 |
|---|---|
| 入力データ | `&Program`, `&Resolution`, `&TypeInfo` |
| 出力データ | `()` |
| 通過関数 | `check_ownership()` → `check_cycles()` → `check_functions()` → `check_borrows()` |
| 生成されるもの | エラーなし（`string` リテラルはムーブなし、型定義の循環もなし） |

`"hello"` は文字列リテラルであり `Copy` 扱い。ムーブも借用も発生しない。

---

## 9. コード生成（`codegen::emit_module()`）

| 項目 | 内容 |
|---|---|
| 入力データ | `&Program`, `&Resolution`, `&TypeInfo` |
| 出力データ | `String`（LLVM IR テキスト） |
| 通過関数 | `emit_module()` → `StructReg::build()` → `emit_function(&main)` → `gen_stmt()` → `gen_expr()` → `gen_call()` |

### 生成される LLVM IR の概要

```llvm
; 文字列グローバル
@.str.0 = private unnamed_addr constant [6 x i8] c"hello\00"

; extern 宣言（prelude から）
declare i32 @puts(ptr)

; @_start エントリポイント（iris が自前生成）
define void @_start() {
entry:
  call void @main()
  ; exit syscall
}

; main 関数
define void @main() {
entry:
  %t0 = call i32 @puts(ptr @.str.0)
  ret void
}
```

`gen_expr()` が `ExprKind::Str("hello")` を受け取り `strings.intern("hello")` を呼んで
`@.str.0` というグローバル記号を返す。`gen_call()` が `call i32 @puts(ptr @.str.0)` を emit する。

---

## 10. clang リンク（`main.rs::build()`）

| 項目 | 内容 |
|---|---|
| 入力データ | IR テキスト（`String`） |
| 出力データ | 実行ファイル（ELF バイナリ） |
| 通過関数 | `std::fs::write(&ll, &ir)` → `Command::new("clang").arg("-O0").arg("-nostartfiles").arg(&ll).arg("-o").arg(&exe)` |
| 生成されるもの | `./hello`（実行可能 ELF） |

```
$ ./hello
hello
```

`-nostartfiles` で CRT を除外し、iris が自前生成した `@_start` がエントリポイントになる。

---

## 全体サマリ

```
hello.iris (ソース)
  ↓ analyze(): プレリュード前置
  ↓ lexer::lex()
Vec<Token>（"hello" → TokenKind::Str）
  ↓ parser::parse()
Program（Stmt::Expr(Call{ callee:puts, args:[Str("hello")] })）
  ↓ sema::resolve()
Resolution（puts の使用 → extern fn puts の定義）
  ↓ sema::check()
TypeInfo（call 式 → Ty::i32、"hello" → Ty::string）
  ↓ sema::check_ownership()
() エラーなし
  ↓ codegen::emit_module()
LLVM IR テキスト（@.str.0 + declare puts + define main）
  ↓ clang -O0 -nostartfiles
ELF 実行ファイル
```
