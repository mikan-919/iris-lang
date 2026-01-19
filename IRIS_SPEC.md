# Language Specification: Iris (v1.1)

Iris は、**Vertical Data-Flow（垂直データフロー）** と **Symbolic Backbone（記号の背骨）** 構文を採用した、Wasm ファーストのシステムプログラミング言語です。

## 1. Core Principles
*   **Verticality**: 主要操作を左端2文字の「背骨」に整列。
*   **Subject-First**: 常に主語から始まり、`=:`（開始）を経て `::`（継続）へ流れる。
*   **Flow Structure vs Procedural Block**: 
    *   **`( )`**: **式構造 (Flow Structure)**。`match` や `join` など、値を生成しパイプラインを分岐・合流させるために使用。
    *   **`{ }`**: **手続きブロック (Procedural Block)**。関数ボディや `mutate`、一時的な変数スコープに使用。
*   **Operator-driven Semantics**: 演算子が実行戦略（非同期、エラー伝播、強制等）を決定。

---

## 2. The Backbone (Operators)
すべての演算子は視覚的一貫性のために2文字幅とする。

| Op | Name | Logic |
| :--- | :--- | :--- |
| **`=:`** | **Initiate** | **開始・束縛**。プロセスを開始し、結果を名前に紐付ける。 |
| **`::`** | **Next** | **継続**。同期的に次の関数へ値を流す。 |
| **`:~`** | **Await** | **待機**。非同期処理の完了を待って次へ流す。 |
| **`:^`** | **Try** | **伝播**。失敗時は即座に return、成功時は次へ。 |
| **`:!`** | **Force** | **断定**。失敗時は Panic、成功時は Unwrap。 |
| **`:?`** | **Catch** | **捕捉**。失敗をハンドルして別の道へ流す。 |
| **`:|`** | **Or** | **代替** | 失敗時に右辺のデフォルト値を流す。 |
| **`:>`** | **Tag** | **借用保存**。値を不変借用として保存し、本流は流す。 |
| **`:&`** | **Join** | **簡易結合**。タプルに値を追加・平坦化する。 |

---

## 3. Syntax & Structure

### Pipeline Definition
```iris
let result
=: initialValue
:: processA()
:> snapshot       // 借用保存
:: processB()      // 所有権移動
```

### Function Styles
```iris
// Procedural Style: 手続きが必要な場合
export fn calculate(input: Int) -> Int {
    let factor =: 10
    input :: * factor :: clamp(0, 100)
}

// Expression Style: 結果を直接定義する場合 (短縮形)
fn double(n: Int) -> Int =: n :: * 2
fn add(a: Int, b: Int) -> Int =: a + b
```

### Control Flow (Flow Structures)
分岐と合流には **`( )`** を使い、各枝の開始は必ず **`=:`** で明示する。これにより「始まり」と「終わり」を視覚化する。

**Match (Branching)**
```iris
let status
=: getResponse()
:: match (
   Ok(user) =:      // 始まり: パターン =:
      :: process(user)
      :: serialize()
   
   Err(e) =:        // 始まり: パターン =:
      :: log(e)
      :| "Error"
) // 終わり: ) で閉じ、背骨のメインラインに戻る
:: print()
```

**Parallelism / Tuple Construction (Join)**
```iris
:: (
   | :: count()           // 親の値を主語として開始
   | =: "Label" :: upper() // 独立した値から開始
) // 結果は (Int, String) のタプル。 ) で合流。
```

---

## 4. Ownership & Mutation
*   **Move by Default**: `::` を通るたびに所有権は移動。
*   **Mutation**: `:: mutate { self.x = 1 }` ブロック内のみ許可。
*   **Borrowing**: `:>` によるタグ付け、または `for` ループの `@` ソース。
