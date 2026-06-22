//! LLVM IR コード生成の回帰テスト。
//!
//! IR テキストの検査は常に実行する。`clang` が使える環境では、生成した IR を
//! 実行ファイルにして終了コードまで検証する（無い環境ではスキップ）。

use std::process::Command;

fn emit(src: &str) -> String {
    iris_lang::compile_ir("test", src).expect("コード生成に成功するはず")
}

fn clang_available() -> bool {
    Command::new("clang")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// IR を clang でコンパイルして実行し、終了コードを返す。clang が無ければ None。
fn run_exit_code(src: &str, tag: &str) -> Option<i32> {
    if !clang_available() {
        return None;
    }
    let ir = emit(src);
    let dir = std::env::temp_dir();
    let ll = dir.join(format!("iris_cg_{tag}.ll"));
    let exe = dir.join(format!("iris_cg_{tag}.bin"));
    std::fs::write(&ll, ir).unwrap();
    let status = Command::new("clang")
        .arg(&ll)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("clang 実行");
    assert!(
        status.status.success(),
        "clang コンパイル失敗:\n{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let run = Command::new(&exe).status().expect("実行");
    let _ = std::fs::remove_file(&ll);
    let _ = std::fs::remove_file(&exe);
    run.code()
}

#[test]
fn emits_function_and_recursion() {
    let ir = emit("fn fac(n: i32): i32 {\n    if n <= 1 {\n        return 1\n    }\n    return n * fac(n - 1)\n}");
    assert!(ir.contains("define i32 @fac(i32 %arg0)"));
    assert!(ir.contains("call i32 @fac"));
    assert!(ir.contains("icmp sle i32"));
    assert!(ir.contains("mul i32"));
}

#[test]
fn emits_short_circuit_for_logical_and() {
    // && は分岐（短絡）で生成される。
    let ir = emit("fn f(a: bool, b: bool): bool {\n    return a && b\n}");
    assert!(ir.contains("br i1"));
    assert!(ir.contains("alloca i1"));
}

#[test]
fn unsupported_type_is_reported() {
    // ジェネリック型（Vec など）はまだコード生成に未対応。
    let err = iris_lang::compile_ir("test", "fn f(v: Vec<i32>): i32 {\n    return 0\n}");
    assert!(err.is_err());
}

#[test]
fn emits_string_literal_as_global() {
    // 文字列リテラルは NUL 終端のグローバル定数になり、値は `ptr` として返る。
    let ir = emit("fn greet(): string {\n    return \"hi\"\n}");
    assert!(ir.contains("define ptr @greet()"));
    assert!(ir.contains(r#"@.str.0 = private unnamed_addr constant [3 x i8] c"hi\00""#));
    assert!(ir.contains("ret ptr @.str.0"));
}

#[test]
fn string_literal_escapes_special_bytes() {
    // 改行・引用符・バックスラッシュは `\XX`（16進）でエスケープされる。
    let ir = emit("fn s(): string {\n    return \"a\\n\\\"b\"\n}");
    // a \n " b \00 = 5 バイト
    assert!(ir.contains(r#"[5 x i8] c"a\0A\22b\00""#));
}

#[test]
fn emits_float_arithmetic() {
    // f64 の算術・比較は浮動小数命令で生成される。
    let ir = emit("fn f(a: f64, b: f64): bool {\n    return a + b == 4.0\n}");
    assert!(ir.contains("define i1 @f(double %arg0, double %arg1)"));
    assert!(ir.contains("fadd double"));
    assert!(ir.contains("fcmp oeq double"));
}

#[test]
fn emits_unsigned_division() {
    // 符号なし整数は udiv/icmp ult を選ぶ。
    let ir = emit("fn f(a: u32, b: u32): bool {\n    return a / b < b\n}");
    assert!(ir.contains("udiv i32"));
    assert!(ir.contains("icmp ult i32"));
}

#[test]
fn emits_wide_integer() {
    // i64 リテラル・演算は i64 幅で生成される。
    let ir = emit("fn f(): i64 {\n    let big: i64 = 5000000000\n    return big + 1\n}");
    assert!(ir.contains("define i64 @f()"));
    assert!(ir.contains("add i64"));
}

#[test]
fn runs_float_computation() {
    // 浮動小数の加算と比較が実行時に正しいこと。
    let src = "fn main(): i32 {\n    let x: f64 = 1.5\n    let y: f64 = 2.5\n    if x + y == 4.0 {\n        return 7\n    }\n    return 0\n}";
    if let Some(code) = run_exit_code(src, "float") {
        assert_eq!(code, 7);
    }
}

#[test]
fn runs_unsigned_division() {
    // 符号なし除算の実行結果。
    let src = "fn main(): i32 {\n    let a: u32 = 200\n    let b: u32 = 7\n    if a / b == 28 {\n        return 9\n    }\n    return 0\n}";
    if let Some(code) = run_exit_code(src, "udiv") {
        assert_eq!(code, 9);
    }
}

#[test]
fn runs_factorial() {
    let src = "fn fac(n: i32): i32 {\n    if n <= 1 {\n        return 1\n    }\n    return n * fac(n - 1)\n}\nfn main(): i32 {\n    return fac(5)\n}";
    if let Some(code) = run_exit_code(src, "fac") {
        assert_eq!(code, 120);
    }
}

#[test]
fn emits_for_counter_loop() {
    // for は cond/body/step/end のカウンタループへ落ちる。
    let ir = emit("fn main(): i32 {\n    let mut s = 0\n    for i in 0..5 {\n        s = s + i\n    }\n    return s\n}");
    assert!(ir.contains("for.cond"));
    assert!(ir.contains("for.step"));
    assert!(ir.contains("for.end"));
    assert!(ir.contains("icmp slt i32"));
    assert!(ir.contains("add i32"));
}

#[test]
fn runs_for_exclusive_range() {
    // 0+1+2+3+4 = 10
    let src = "fn main(): i32 {\n    let mut s = 0\n    for i in 0..5 {\n        s = s + i\n    }\n    return s\n}";
    if let Some(code) = run_exit_code(src, "for_excl") {
        assert_eq!(code, 10);
    }
}

#[test]
fn runs_for_inclusive_range() {
    // 1+2+3+4+5 = 15
    let src = "fn main(): i32 {\n    let mut s = 0\n    for i in 1..=5 {\n        s = s + i\n    }\n    return s\n}";
    if let Some(code) = run_exit_code(src, "for_incl") {
        assert_eq!(code, 15);
    }
}

#[test]
fn runs_for_with_break_and_continue() {
    // continue で偶数を飛ばし 0..10 の奇数和 25、break で 3 で打ち切り。
    let src = "fn main(): i32 {\n    let mut odd = 0\n    for i in 0..10 {\n        if i % 2 == 0 {\n            continue\n        }\n        odd = odd + i\n    }\n    let mut c = 0\n    for k in 0..100 {\n        if k == 3 {\n            break\n        }\n        c = c + 1\n    }\n    return odd + c\n}";
    if let Some(code) = run_exit_code(src, "for_brk_cont") {
        assert_eq!(code, 28); // 25 + 3
    }
}

#[test]
fn runs_nested_for_with_variable_bound() {
    // 3x3 の二重ループ = 9。境界は変数。
    let src = "fn main(): i32 {\n    let n = 3\n    let mut grid = 0\n    for a in 0..n {\n        for b in 0..n {\n            grid = grid + 1\n        }\n    }\n    return grid\n}";
    if let Some(code) = run_exit_code(src, "for_nested") {
        assert_eq!(code, 9);
    }
}

#[test]
fn runs_arithmetic_and_let() {
    let src = "fn main(): i32 {\n    let a = 6\n    let mut b = 7\n    b = b * a\n    return b - 2\n}";
    if let Some(code) = run_exit_code(src, "arith") {
        assert_eq!(code, 40);
    }
}

#[test]
fn runs_if_else_and_comparison() {
    let src = "fn classify(n: i32): i32 {\n    if n > 10 {\n        return 2\n    } else {\n        return 1\n    }\n}\nfn main(): i32 {\n    return classify(5) + classify(20)\n}";
    if let Some(code) = run_exit_code(src, "ifelse") {
        assert_eq!(code, 3);
    }
}

#[test]
fn runs_ternary_and_short_circuit() {
    let src = "fn main(): i32 {\n    let x = 5\n    let ok = x > 0 && x < 10\n    return ok ? x * 2 : 0\n}";
    if let Some(code) = run_exit_code(src, "tern") {
        assert_eq!(code, 10);
    }
}

#[test]
fn emits_reference_params_as_ptr() {
    // 参照は opaque ポインタ。値として使うときは暗黙にデリファレンス（load）する。
    let ir = emit("fn add(a: &i32, b: &i32): i32 {\n    return a + b\n}");
    assert!(ir.contains("define i32 @add(ptr %arg0, ptr %arg1)"));
    // 参照そのもの（ptr）を読み、続けて指す先の i32 を読む二段 load。
    assert!(ir.contains("load ptr, ptr"));
    assert!(ir.contains("load i32, ptr"));
    assert!(ir.contains("add i32"));
}

#[test]
fn runs_reference_deref_sum() {
    // `&x` で場所のアドレスを渡し、関数側で暗黙 deref して合計する。
    let src = "fn add(a: &i32, b: &i32): i32 {\n    return a + b\n}\nfn main(): i32 {\n    let x = 20\n    let y = 22\n    return add(&x, &y)\n}";
    if let Some(code) = run_exit_code(src, "ref_sum") {
        assert_eq!(code, 42);
    }
}

#[test]
fn runs_reference_local_binding() {
    // 参照をローカルに束縛し、値の文脈で使うと暗黙にデリファレンスされる。
    let src = "fn main(): i32 {\n    let x = 7\n    let r = &x\n    return r + 1\n}";
    if let Some(code) = run_exit_code(src, "ref_local") {
        assert_eq!(code, 8);
    }
}

#[test]
fn runs_mut_ref_and_bool_ref_condition() {
    // `&mut` の引き渡しと、`&bool` の条件での暗黙 deref。
    let src = "fn pick(flag: &bool, a: &i32, b: &i32): i32 {\n    if flag {\n        return a\n    }\n    return b\n}\nfn main(): i32 {\n    let cond = true\n    let mut x = 30\n    let y = 12\n    return pick(&cond, &mut x, &y)\n}";
    if let Some(code) = run_exit_code(src, "ref_mut_bool") {
        assert_eq!(code, 30);
    }
}

#[test]
fn emits_while_loop_blocks() {
    let ir = emit("fn main(): i32 {\n    let mut i = 0\n    while i < 3 {\n        i = i + 1\n    }\n    return i\n}");
    assert!(ir.contains("while.cond"));
    assert!(ir.contains("while.body"));
    assert!(ir.contains("while.end"));
    // 条件への分岐（後方辺）と本体/末尾への条件分岐。
    assert!(ir.contains("br i1"));
}

#[test]
fn runs_while_sum() {
    // 1..=5 の合計 = 15。
    let src = "fn main(): i32 {\n    let mut i = 1\n    let mut acc = 0\n    while i <= 5 {\n        acc = acc + i\n        i = i + 1\n    }\n    return acc\n}";
    if let Some(code) = run_exit_code(src, "while_sum") {
        assert_eq!(code, 15);
    }
}

#[test]
fn runs_loop_with_break() {
    // 100 を超えない最大の 2 のべき乗 = 64。
    let src = "fn main(): i32 {\n    let mut x = 1\n    loop {\n        if x * 2 > 100 {\n            break\n        }\n        x = x * 2\n    }\n    return x\n}";
    if let Some(code) = run_exit_code(src, "loop_break") {
        assert_eq!(code, 64);
    }
}

#[test]
fn emits_named_struct_type_and_gep() {
    // struct は名前付き LLVM 構造体型として宣言し、フィールドは GEP で読み書きする。
    let ir = emit(
        "type Point = struct {\n    x: i32\n    y: i32\n}\nfn sum(p: Point): i32 {\n    return p.x + p.y\n}",
    );
    assert!(ir.contains("%Point = type { i32, i32 }"));
    assert!(ir.contains("define i32 @sum(%Point %arg0)"));
    assert!(ir.contains("getelementptr inbounds %Point, ptr"));
}

#[test]
fn emits_struct_literal_construction() {
    // 構造体リテラルは alloca + 各フィールド store + 全体 load で first-class 値を作る。
    let ir = emit(
        "type Point = struct {\n    x: i32\n    y: i32\n}\nfn make(): Point {\n    return Point { x: 1, y: 2 }\n}",
    );
    assert!(ir.contains("alloca %Point"));
    assert!(ir.contains("store i32 1, ptr"));
    assert!(ir.contains("load %Point, ptr"));
    assert!(ir.contains("ret %Point"));
}

#[test]
fn runs_struct_literal_and_member_access() {
    // 値で渡した struct のフィールド合計。
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\nfn sum(p: Point): i32 {\n    return p.x + p.y\n}\nfn main(): i32 {\n    let p = Point { x: 17, y: 25 }\n    return sum(p)\n}";
    if let Some(code) = run_exit_code(src, "struct_sum") {
        assert_eq!(code, 42);
    }
}

#[test]
fn runs_struct_reference_member_access() {
    // 参照越しのメンバアクセス（暗黙にポインタを辿る）。
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\nfn shift(p: &Point): i32 {\n    return p.x * 10 + p.y\n}\nfn main(): i32 {\n    let p = Point { x: 4, y: 2 }\n    return shift(&p)\n}";
    if let Some(code) = run_exit_code(src, "struct_ref") {
        assert_eq!(code, 42);
    }
}

#[test]
fn runs_struct_field_assignment() {
    // ローカル struct のフィールドへの代入 `q.x = v`。
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\nfn main(): i32 {\n    let mut q = Point { x: 1, y: 2 }\n    q.x = 40\n    return q.x + q.y\n}";
    if let Some(code) = run_exit_code(src, "struct_assign") {
        assert_eq!(code, 42);
    }
}

#[test]
fn emits_write_through_store() {
    // `&mut i32` への代入は、参照値（ptr）を load してその指す先へ store する（rebind ではない）。
    let ir = emit("fn set(r: &mut i32) {\n    r = 99\n}");
    assert!(ir.contains("define void @set(ptr %arg0)"));
    // r（ptr）を読み出し、その先へ i32 を書き込む。
    assert!(ir.contains("load ptr, ptr %r.addr"));
    assert!(ir.contains("store i32 99, ptr %t0"));
}

#[test]
fn runs_write_through_mut_ref() {
    // 参照越し代入で呼び出し側のローカルが書き換わる。
    let src = "fn set(r: &mut i32) {\n    r = 99\n}\nfn main(): i32 {\n    let mut x = 1\n    set(&mut x)\n    return x\n}";
    if let Some(code) = run_exit_code(src, "write_through") {
        assert_eq!(code, 99);
    }
}

#[test]
fn runs_write_through_increment() {
    // `r = r + 1`: 右辺で r を auto-deref して読み、その値+1 を参照先へ書き戻す。
    let src = "fn add_one(r: &mut i32) {\n    r = r + 1\n}\nfn main(): i32 {\n    let mut x = 41\n    add_one(&mut x)\n    return x\n}";
    if let Some(code) = run_exit_code(src, "write_through_inc") {
        assert_eq!(code, 42);
    }
}

#[test]
fn runs_nested_struct_and_call_return_member() {
    // ネストした struct のチェーンアクセスと、関数戻り値（場所でない値）のメンバアクセス。
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\ntype Line = struct {\n    a: Point\n    b: Point\n}\nfn origin(): Point {\n    return Point { x: 2, y: 5 }\n}\nfn main(): i32 {\n    let l = Line { a: Point { x: 1, y: 2 }, b: Point { x: 3, y: 4 } }\n    return l.a.x * 10 + l.b.y + origin().y\n}";
    if let Some(code) = run_exit_code(src, "struct_nested") {
        assert_eq!(code, 19);
    }
}

#[test]
fn runs_while_with_continue() {
    // 1..=10 のうち偶数の合計 = 30。
    let src = "fn main(): i32 {\n    let mut i = 0\n    let mut acc = 0\n    while i < 10 {\n        i = i + 1\n        if i % 2 == 1 {\n            continue\n        }\n        acc = acc + i\n    }\n    return acc\n}";
    if let Some(code) = run_exit_code(src, "while_cont") {
        assert_eq!(code, 30);
    }
}

#[test]
fn runs_enum_match_no_payload() {
    // ペイロードなし enum と match 式の縦断テスト。
    // Red=1, Green=2, Blue=3 → color_code(Green) = 2。
    let src = "type Color = enum {\n    Red\n    Green\n    Blue\n}\nfn color_code(c: Color): i32 {\n    match c {\n        Red -> 1\n        Green -> 2\n        Blue -> 3\n        _ -> 0\n    }\n}\nfn main(): i32 {\n    return color_code(Green)\n}";
    if let Some(code) = run_exit_code(src, "enum_no_payload") {
        assert_eq!(code, 2);
    }
}

#[test]
fn runs_enum_match_with_payload() {
    // ペイロードあり enum: Rect(5) → side * side = 25。
    let src = "type Shape = enum {\n    Circle\n    Rect(i32)\n}\nfn area(s: Shape): i32 {\n    match s {\n        Circle -> 0\n        Rect(side) -> side * side\n        _ -> -1\n    }\n}\nfn main(): i32 {\n    return area(Rect(5))\n}";
    if let Some(code) = run_exit_code(src, "enum_payload") {
        assert_eq!(code, 25);
    }
}

#[test]
fn runs_enum_match_wildcard_fallthrough() {
    // ワイルドカードアームが正しく機能する。
    let src = "type Dir = enum {\n    North\n    South\n    East\n    West\n}\nfn is_north(d: Dir): i32 {\n    match d {\n        North -> 1\n        _ -> 0\n    }\n}\nfn main(): i32 {\n    let a = is_north(North)\n    let b = is_north(South)\n    return a * 10 + b\n}";
    if let Some(code) = run_exit_code(src, "enum_wildcard") {
        assert_eq!(code, 10);
    }
}

#[test]
fn emits_strcmp_for_string_pattern() {
    // 文字列リテラルパターンは strcmp 呼び出しで比較し、結果を 0 と比べる。
    let ir = emit(
        "fn r(s: string): i32 {\n    match s {\n        \"hi\" -> 1\n        _ -> 0\n    }\n}",
    );
    assert!(ir.contains("declare i32 @strcmp(ptr, ptr)"));
    assert!(ir.contains("call i32 @strcmp(ptr"));
    assert!(ir.contains("icmp eq i32"));
}

#[test]
fn runs_string_pattern_match() {
    // 文字列パターンの分岐: rank("silver")=2, rank("none")=0 → 2*10+0 = 20。
    let src = "fn rank(s: string): i32 {\n    match s {\n        \"gold\" -> 3\n        \"silver\" -> 2\n        \"bronze\" -> 1\n        _ -> 0\n    }\n}\nfn main(): i32 {\n    return rank(\"silver\") * 10 + rank(\"none\")\n}";
    if let Some(code) = run_exit_code(src, "match_str") {
        assert_eq!(code, 20);
    }
}

#[test]
fn emits_range_pattern_comparison() {
    // 範囲パターンは下限・上限の 2 比較を and で合成する（排他は slt、包含は sle）。
    let ir = emit(
        "fn g(n: i32): i32 {\n    match n {\n        0..10 -> 1\n        10..=20 -> 2\n        _ -> 0\n    }\n}",
    );
    assert!(ir.contains("icmp sge i32"));
    assert!(ir.contains("icmp slt i32"));
    assert!(ir.contains("icmp sle i32"));
    assert!(ir.contains("and i1"));
}

#[test]
fn runs_range_pattern_match() {
    // 排他 `..` と包含 `..=`: grade(45)=1, grade(60)=2, grade(79)=2, grade(80)=3, grade(100)=3。
    // 1 + 2*10 + 2*100 + 3*1000 + 3*10000 = 33221、終了コードは下位 8 ビット = 197。
    let src = "fn grade(n: i32): i32 {\n    match n {\n        0..60 -> 1\n        60..=79 -> 2\n        80..=100 -> 3\n        _ -> 0\n    }\n}\nfn main(): i32 {\n    return grade(45) + grade(60) * 10 + grade(79) * 100 + grade(80) * 1000 + grade(100) * 10000\n}";
    if let Some(code) = run_exit_code(src, "match_range") {
        assert_eq!(code, 197);
    }
}

#[test]
fn runs_option_some_match() {
    // builtin Option<i32> の構築と match の縦断テスト: Some(42) を unwrap して 42。
    let src = "fn unwrap_or_zero(o: Option<i32>): i32 {\n    match o {\n        Some(n) -> n\n        None -> 0\n    }\n}\nfn main(): i32 {\n    return unwrap_or_zero(Some(42))\n}";
    if let Some(code) = run_exit_code(src, "option_some") {
        assert_eq!(code, 42);
    }
}

#[test]
fn runs_result_ok_err_match() {
    // builtin Result<i32, i32>: Ok/Err の双方を構築・match する。
    // handle(Ok(7))=7, handle(Err(3))=3 → 7*10 + 3 = 73。
    let src = "fn handle(r: Result<i32, i32>): i32 {\n    match r {\n        Ok(v) -> v\n        Err(e) -> e\n    }\n}\nfn main(): i32 {\n    return handle(Ok(7)) * 10 + handle(Err(3))\n}";
    if let Some(code) = run_exit_code(src, "result_ok_err") {
        assert_eq!(code, 73);
    }
}

#[test]
fn runs_user_generic_enum_match() {
    // ユーザ定義のジェネリック enum: Pair<i32> を構築・match する。
    // pick(First(5))=5, pick(Second(3))=6 → 5*10 + 6 = 56。
    let src = "type Pair<T> = enum {\n    First(T)\n    Second(T)\n}\nfn pick(p: Pair<i32>): i32 {\n    match p {\n        First(x) -> x\n        Second(y) -> y * 2\n    }\n}\nfn main(): i32 {\n    return pick(First(5)) * 10 + pick(Second(3))\n}";
    if let Some(code) = run_exit_code(src, "user_generic_enum") {
        assert_eq!(code, 56);
    }
}

#[test]
fn runs_generic_enum_struct_payload() {
    // 集約ペイロード: Option<Point>（struct）を構築・match してフィールドを取り出す。
    // per-instantiation レイアウト `%"Option$Point" = type { i8, %Point }` を発行する。
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\nfn get_x(o: Option<Point>): i32 {\n    match o {\n        Some(p) -> p.x\n        None -> 0\n    }\n}\nfn main(): i32 {\n    return get_x(Some(Point { x: 9, y: 4 }))\n}";
    if let Some(code) = run_exit_code(src, "enum_struct_payload") {
        assert_eq!(code, 9);
    }
}

#[test]
fn runs_user_generic_enum_struct_payload() {
    // ユーザ定義ジェネリック enum × 集約ペイロード: Wrap<Point>。
    // get(Holds(Point{3,4})) → 3 + 4 = 7。
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\ntype Wrap<T> = enum {\n    Empty\n    Holds(T)\n}\nfn get(w: Wrap<Point>): i32 {\n    match w {\n        Holds(p) -> p.x + p.y\n        Empty -> 0 - 1\n    }\n}\nfn main(): i32 {\n    return get(Holds(Point { x: 3, y: 4 }))\n}";
    if let Some(code) = run_exit_code(src, "user_enum_struct_payload") {
        assert_eq!(code, 7);
    }
}

#[test]
fn runs_result_error_propagation() {
    // `!` で Result を伝播。run(5): parse(5)=Ok(10) → v=10 → Ok(11)。
    // run(-3): parse(-3)=Err(-3) → run が Err(-3) を早期 return。
    // ra=11, rb=-3 → 11*10 + (0 - (-3)) = 113。
    let src = "fn parse(n: i32): Result<i32, i32> {\n    if n < 0 {\n        return Err(n)\n    }\n    return Ok(n * 2)\n}\nfn run(n: i32): Result<i32, i32> {\n    let v = parse(n)!\n    return Ok(v + 1)\n}\nfn main(): i32 {\n    let a = run(5)\n    let b = run(0 - 3)\n    let ra = match a {\n        Ok(x) -> x\n        Err(e) -> 0 - 1\n    }\n    let rb = match b {\n        Ok(x) -> x\n        Err(e) -> e\n    }\n    return ra * 10 + (0 - rb)\n}";
    if let Some(code) = run_exit_code(src, "try_result") {
        assert_eq!(code, 113);
    }
}

#[test]
fn runs_option_none_propagation() {
    // `!` で Option を伝播。doubled(5): first_pos(5)=Some(5) → v=5 → Some(10)。
    // doubled(0): first_pos(0)=None → doubled が None を早期 return。
    // ra=10, rb=99 → 109。
    let src = "fn first_pos(n: i32): Option<i32> {\n    if n > 0 {\n        return Some(n)\n    }\n    return None\n}\nfn doubled(n: i32): Option<i32> {\n    let v = first_pos(n)!\n    return Some(v * 2)\n}\nfn main(): i32 {\n    let a = doubled(5)\n    let b = doubled(0)\n    let ra = match a {\n        Some(x) -> x\n        None -> 0\n    }\n    let rb = match b {\n        Some(x) -> x\n        None -> 99\n    }\n    return ra + rb\n}";
    if let Some(code) = run_exit_code(src, "try_option") {
        assert_eq!(code, 109);
    }
}

#[test]
fn runs_result_struct_error_propagation() {
    // 集約（struct）エラーペイロードの伝播: Result<i32, Fail>。
    // run(5)=Ok(105)。run(-7): check(-7)=Err(Fail{7}) → run が Err を再構築して return。
    // ra=105, rb=7 → 112。
    let src = "type Fail = struct {\n    code: i32\n}\nfn check(n: i32): Result<i32, Fail> {\n    if n < 0 {\n        return Err(Fail { code: 0 - n })\n    }\n    return Ok(n)\n}\nfn run(n: i32): Result<i32, Fail> {\n    let v = check(n)!\n    return Ok(v + 100)\n}\nfn main(): i32 {\n    let a = run(5)\n    let b = run(0 - 7)\n    let ra = match a {\n        Ok(x) -> x\n        Err(f) -> 0\n    }\n    let rb = match b {\n        Ok(x) -> x\n        Err(f) -> f.code\n    }\n    return ra + rb\n}";
    if let Some(code) = run_exit_code(src, "try_struct_err") {
        assert_eq!(code, 112);
    }
}

#[test]
fn emits_guard_branch() {
    // ガード付きアームは束縛設定の後にガードを評価し、専用ブロックへ分岐する。
    let ir = emit(
        "type Shape = enum {\n    Circle\n    Rect(i32)\n}\nfn f(s: Shape): i32 {\n    match s {\n        Rect(side) if side > 10 -> 2\n        _ -> 0\n    }\n}",
    );
    assert!(ir.contains("match.guarded"));
}

#[test]
fn runs_match_guard() {
    // ペイロード束縛を参照するガードと、scrutinee を参照するワイルドカードガード。
    // classify(Rect(20))=3, classify(Rect(5))=2, classify(Rect(0))=1 → 321。
    // pick(0)=200（0 アーム）, pick(5)=300（_ アーム）→ 321 + 200 - 300 = 221。
    let src = "type Shape = enum {\n    Circle\n    Rect(i32)\n}\nfn classify(s: Shape): i32 {\n    match s {\n        Rect(side) if side > 10 -> 3\n        Rect(side) if side > 0 -> 2\n        Rect(side) -> 1\n        Circle -> 0\n        _ -> -1\n    }\n}\nfn pick(n: i32): i32 {\n    match n {\n        _ if n < 0 -> 100\n        0 -> 200\n        _ -> 300\n    }\n}\nfn main(): i32 {\n    let a = classify(Rect(20))\n    let b = classify(Rect(5))\n    let c = classify(Rect(0))\n    return a * 100 + b * 10 + c + pick(0) - pick(5)\n}";
    if let Some(code) = run_exit_code(src, "match_guard") {
        assert_eq!(code, 221);
    }
}

#[test]
fn runs_for_in_iterator() {
    // 一般イテレータ `for x in it`（ADR-0007）。Counter が Iterator<i32> を実装し、
    // next() で 0,1,2,3,4 を Some で返し、5 で None。for で総和 0+1+2+3+4 = 10。
    let src = "type Counter = struct {\n    cur: i32\n    hi: i32\n}\nimpl Iterator<i32> for Counter {\n    fn next(&mut self): Option<i32> {\n        if self.cur < self.hi {\n            let v = self.cur\n            self.cur = self.cur + 1\n            return Some(v)\n        }\n        return None\n    }\n}\nfn main(): i32 {\n    let mut c = Counter { cur: 0, hi: 5 }\n    let mut sum = 0\n    for x in c {\n        sum = sum + x\n    }\n    return sum\n}";
    if let Some(code) = run_exit_code(src, "for_in_iter") {
        assert_eq!(code, 10);
    }
}

#[test]
fn runs_for_in_break_continue() {
    // for-in 内の break / continue。Counter は 0,1,2,... を返す。
    // x==2 で continue（飛ばす）、x==5 で break（終了）。和 = 0+1+3+4 = 8。
    let src = "type Counter = struct {\n    cur: i32\n    hi: i32\n}\nimpl Iterator<i32> for Counter {\n    fn next(&mut self): Option<i32> {\n        if self.cur < self.hi {\n            let v = self.cur\n            self.cur = self.cur + 1\n            return Some(v)\n        }\n        return None\n    }\n}\nfn main(): i32 {\n    let mut c = Counter { cur: 0, hi: 100 }\n    let mut sum = 0\n    for x in c {\n        if x == 5 {\n            break\n        }\n        if x == 2 {\n            continue\n        }\n        sum = sum + x\n    }\n    return sum\n}";
    if let Some(code) = run_exit_code(src, "for_in_brk") {
        assert_eq!(code, 8);
    }
}

#[test]
fn runs_for_in_struct_element() {
    // 集約ペイロード: Iterator<Point> を for-in で回す（Option<Point> の per-instantiation
    // レイアウト・ADR-0010）。Gen は (0,10),(1,11),(2,12) を返す。和 = 36。
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\ntype Gen = struct {\n    cur: i32\n    hi: i32\n}\nimpl Iterator<Point> for Gen {\n    fn next(&mut self): Option<Point> {\n        if self.cur < self.hi {\n            let v = self.cur\n            self.cur = self.cur + 1\n            return Some(Point { x: v, y: v + 10 })\n        }\n        return None\n    }\n}\nfn main(): i32 {\n    let mut g = Gen { cur: 0, hi: 3 }\n    let mut sum = 0\n    for p in g {\n        sum = sum + p.x + p.y\n    }\n    return sum\n}";
    if let Some(code) = run_exit_code(src, "for_in_struct") {
        assert_eq!(code, 36);
    }
}

#[test]
fn runs_generic_struct() {
    // ジェネリック struct の単相化。Pair<T> を i32 で具体化し、フィールド和を返す。
    // a=3, b=4 → 7。
    let src = "type Pair<T> = struct {\n    a: T\n    b: T\n}\nfn main(): i32 {\n    let p = Pair { a: 3, b: 4 }\n    return p.a + p.b\n}";
    if let Some(code) = run_exit_code(src, "generic_struct") {
        assert_eq!(code, 7);
    }
}

#[test]
fn runs_generic_struct_through_fn() {
    // ジェネリック struct を関数引数として値渡し（シグネチャ経由でインスタンスを収集）。
    // Pair<i32> を作って渡し、p.a*10 + p.b = 42。
    let src = "type Pair<T> = struct {\n    a: T\n    b: T\n}\nfn combine(p: Pair<i32>): i32 {\n    return p.a * 10 + p.b\n}\nfn main(): i32 {\n    let p = Pair { a: 4, b: 2 }\n    return combine(p)\n}";
    if let Some(code) = run_exit_code(src, "gstruct_fn") {
        assert_eq!(code, 42);
    }
}

#[test]
fn runs_generic_struct_two_params_two_insts() {
    // 2 つの型パラメータ・2 つの異なるインスタンス（Box2<i32,bool> と Box2<i32,i32>）。
    // a.second ? a.first : 0 = 30、b.second = 5 → 35。
    let src = "type Box2<A, B> = struct {\n    first: A\n    second: B\n}\nfn main(): i32 {\n    let a = Box2 { first: 30, second: true }\n    let b = Box2 { first: 12, second: 5 }\n    let s = a.second ? a.first : 0\n    return s + b.second\n}";
    if let Some(code) = run_exit_code(src, "gstruct_two") {
        assert_eq!(code, 35);
    }
}

#[test]
fn runs_generic_struct_struct_field() {
    // ジェネリック struct のフィールドが具象 struct（%Wrap.Point = type { %Point }）。
    // w.val.x + w.val.y = 5 + 7 = 12。
    let src = "type Point = struct {\n    x: i32\n    y: i32\n}\ntype Wrap<T> = struct {\n    val: T\n}\nfn main(): i32 {\n    let w = Wrap { val: Point { x: 5, y: 7 } }\n    return w.val.x + w.val.y\n}";
    if let Some(code) = run_exit_code(src, "gstruct_field") {
        assert_eq!(code, 12);
    }
}

#[test]
fn runs_option_of_generic_struct() {
    // 集約 enum × ジェネリック struct: Option<Pair<i32>>（payload = %Pair.i32）。
    // Some(Pair{8,9}) を match で取り出し p.a + p.b = 17。
    let src = "type Pair<T> = struct {\n    a: T\n    b: T\n}\nfn first(o: Option<Pair<i32>>): i32 {\n    match o {\n        Some(p) -> p.a + p.b\n        None -> 0\n    }\n}\nfn main(): i32 {\n    let o = Some(Pair { a: 8, b: 9 })\n    return first(o)\n}";
    if let Some(code) = run_exit_code(src, "opt_gstruct") {
        assert_eq!(code, 17);
    }
}
