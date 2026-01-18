
# Language Specification: Iris (v1.0)

Iris is a Wasm-first, performance-oriented language designed for maximum readability through **Vertical Data-Flow** and **Symbolic Backbone** syntax.

## 1. Core Principles
*   **Verticality**: All major operations align to the left in a 2-character "backbone".
*   **Subject-First**: Data flows from a Subject (result) through an Initiation (`=:`) into a Pipeline (`::`).
*   **Ownership by Flow**: Ownership is moved by default as data descends the pipeline.
*   **Operator-driven Semantics**: The operator defines the execution strategy (Async, Try, Force), keeping function names clean.

---

## 2. The Backbone (Operators)
All operators must be 2 characters wide to maintain the vertical visual line.

| Op | Name | Logic | Memory/Control Flow |
| :--- | :--- | :--- | :--- |
| **`=:`** | **Bind** | Assignment / Start | Moves the pipeline result into the variable. |
| **`::`** | **Next** | Standard Map | Moves value to the next function (Sync). |
| **`:~`** | **Await** | Async Call | Awaits the Promise/Future before proceeding. |
| **`:^`** | **Try** | Error Propagate | Returns `Err` to caller if failed; else unwraps. |
| **`:!`** | **Force** | Assert/Unwrap | Panics if failed; else unwraps. |
| **`:?`** | **Catch** | Error Rescue | Branches into an error handler on `Err`. |
| **`:|`** | **Or** | Fallback | Forwards a default value if `None`/`Err`. |
| **`:>`** | **Tag** | Borrow/Export | Stores an immutable reference; continues flow. |
| **`:&`** | **Join** | Tuple Merge | Flattens and appends value into a tuple. |

---

## 3. Syntax & Structure

### Pipeline Definition
```iris
let result
=: initialValue
:: processA()
:> snapshot       // Borrowed reference stored in 'snapshot'
:: processB()     // Ownership moves to B
```

### Functions
Iris supports two styles for function definitions: Procedural Style and Expression Style.

**Procedural Style** (`fn { ... }`): For complex logic with multiple statements.
```iris
export fn calculate(input: Int) -> Int {
    let factor =: 10
    input :: * factor :: clamp(0, 100)
}
```

**Expression Style** (`fn =: ...`): For simple functions that return a single expression.
```iris
fn double(n: Int) -> Int =: n :: * 2
fn add(a: Int, b: Int) -> Int =: a + b
fn greet(name: String) -> String =: "Hello, " :: concat(name)
fn get_constant() -> Int =: 42
```

### Control Flow (No `if`, No `while`)
**Match** (No arrows `=>`, use `::`)
```iris
:: match
   | PatternA :: handleA()
   | PatternB :: handleB()
   | _        :! "Error message"
```

**Loops** (`@` denotes "at" the collection)
```iris
// Threading Loop (State stays as subject, iterates over external source)
canvas :: for shape@shapes :: draw(shape)

// Exploding Loop (Consumes and unpacks subject)
users :: for user :: process(user)
```

**Parallelism / Tuple Construction**
```iris
:: (
   | :: count()           // Fork from parent subject
   | "Label" :: toUpper() // Gather from new source
) // Results in (Int, String)
```

---

## 4. Ownership & Memory Model
Iris is GC-free by default, using Linear Types and Flow Analysis.

*   **Move by Default**: Passing a non-`Copy` value to `::` moves ownership.
*   **Mutation**: Only allowed within `:: mutate { self.x = 1 }` blocks.
*   **Borrowing**: Occurs via `:>` or when passing values to loop sources (the `@` part).
*   **Shared Ownership**: Opt-in via `:: share()` which wraps value in an `Rc<T>`.

---

## 5. Data Structures
```iris
struct User {
    id: Int
    name: String
}

enum Status
| Active
| Banned(String)
| Pending { since: Date }
```

---

## 6. FFI & Modules
*   **Import**: Standard JS-like syntax `import { x } from "mod"`.
*   **Export**: Prefix with `export`. No `default export`.
*   **Binding**: Functions can bind directly to host symbols via string literals.

```iris
export fn alert(msg: String) =: "window.alert"
```

---

## 7. Compiler Requirements (Internal)
*   **Target**: WebAssembly (Wasm).
*   **Frontend**: Parser must handle 2-char backbone and indentation for `match`/`for`/`join`.
*   **Analysis**:
    1. Hindley-Milner Type Inference.
    2. Data-flow Ownership Tracking (prevent double-use).
    3. Escape Analysis for Stack-to-Heap promotion.
*   **Optimizations**: Refcount elision, Inline monomorphization, Wasm SIMD.
