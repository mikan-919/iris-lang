//! 型検査パスの回帰テスト。

use iris_lang::lexer::lex;
use iris_lang::parser::parse;
use iris_lang::sema::{check, resolve};

/// 型検査まで通す。エラーがあればメッセージ一覧を返す。
fn typecheck(src: &str) -> Result<(), Vec<String>> {
    let tokens = lex(src).expect("字句解析");
    let program = parse(&tokens).expect("構文解析");
    let resolution = resolve(&program, Default::default()).expect("名前解決");
    check(&program, &resolution)
        .map(|_| ())
        .map_err(|errs| errs.into_iter().map(|e| e.message).collect())
}

#[test]
fn accepts_well_typed_program() {
    let src = "fn add(a: i32, b: i32): i32 {\n    let s = a + b\n    return s\n}";
    typecheck(src).expect("型が整合しているはず");
}

#[test]
fn rejects_let_annotation_mismatch() {
    let errs = typecheck("fn f() {\n    let x: i32 = true\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("型が一致しません")));
}

#[test]
fn allows_int_literal_into_any_integer() {
    typecheck("fn f() {\n    let x: i64 = 1\n    let y: u8 = 2\n}").expect("リテラルは整数型へ適合");
}

#[test]
fn rejects_concrete_int_width_mismatch() {
    // i32 の変数を i64 へ代入はできない（リテラルではない具体型同士）。
    let errs = typecheck("fn f() {\n    let a: i32 = 1\n    let b: i64 = a\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("型が一致しません")));
}

#[test]
fn rejects_arithmetic_on_bool() {
    let errs = typecheck("fn f() {\n    let x = true + 1\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("算術演算")));
}

#[test]
fn comparison_yields_bool() {
    typecheck("fn f() {\n    let b: bool = 1 < 2\n}").expect("比較は bool");
}

#[test]
fn rejects_return_type_mismatch() {
    let errs = typecheck("fn f(): i32 {\n    return true\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("戻り値の型")));
}

#[test]
fn checks_call_arity_and_args() {
    let arity = typecheck("fn g(a: i32) {\n}\nfn f() {\n    g(1, 2)\n}").unwrap_err();
    assert!(arity.iter().any(|m| m.contains("引数は")));

    let argty = typecheck("fn g(a: i32) {\n}\nfn f() {\n    g(true)\n}").unwrap_err();
    assert!(argty.iter().any(|m| m.contains("引数の型")));
}

#[test]
fn try_requires_result_returning_fn() {
    // void を返す関数で `!` は使えない。
    let errs =
        typecheck("fn p(): Result<i32, string> {\n    return Ok(1)\n}\nfn f() {\n    let x = p()!\n}")
            .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("Result または Option を返す")));
}

#[test]
fn try_unwraps_result() {
    let src = "fn p(): Result<i32, string> {\n    return Ok(1)\n}\nfn f(): Result<i32, string> {\n    let x: i32 = p()!\n    return Ok(x)\n}";
    typecheck(src).expect("`!` は Result の中身 i32 を取り出す");
}

#[test]
fn rejects_try_kind_mismatch() {
    // Result を返す関数で Option に `!` は使えない（伝播する値が無い）。
    let errs = typecheck(
        "fn o(): Option<i32> {\n    return Some(1)\n}\nfn f(): Result<i32, string> {\n    let x = o()!\n    return Ok(x)\n}",
    )
    .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("伝播できません")));
}

#[test]
fn rejects_try_on_non_result() {
    let errs = typecheck("fn f(): Result<i32, string> {\n    let x = 1!\n    return Ok(0)\n}")
        .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("Result または Option にのみ")));
}

#[test]
fn rejects_reassign_immutable() {
    let errs = typecheck("fn f() {\n    let x = 1\n    x = 2\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("再代入できません")));
}

#[test]
fn allows_reassign_mutable() {
    typecheck("fn f() {\n    let mut x = 1\n    x = 2\n}").expect("let mut は再代入可");
}

#[test]
fn write_through_mut_ref_typechecks() {
    // `&mut T` への値型 T の代入は参照越し書き込み（write-through）。inner 型で判定。
    typecheck("fn set(r: &mut i32) {\n    r = 99\n}").expect("&mut への write-through は通る");
}

#[test]
fn rejects_write_through_immutable_ref() {
    // 不変参照 `&T` の参照先には書き込めない。
    let errs = typecheck("fn set(r: &i32) {\n    r = 99\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("不変参照")));
}

#[test]
fn rejects_write_through_inner_type_mismatch() {
    // write-through は参照の inner 型と右辺を突き合わせる（`&mut i32` に bool は不可）。
    let errs = typecheck("fn set(r: &mut i32) {\n    r = true\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("型が一致しません")));
}

#[test]
fn ref_rebind_still_typechecks() {
    // 右辺が参照型なら write-through ではなく束縛の付け替え（rebind）。target 全体で判定。
    typecheck("fn f() {\n    let a = 1\n    let b = 2\n    let mut r = &a\n    r = &b\n}")
        .expect("参照の付け替えは通る");
}

#[test]
fn rejects_ternary_branch_mismatch() {
    let errs = typecheck("fn f(c: bool) {\n    let x = c ? 1 : true\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("三項演算子の分岐")));
}

#[test]
fn accepts_while_with_bool_condition() {
    typecheck("fn f() {\n    let mut i = 0\n    while i < 3 {\n        i = i + 1\n    }\n}")
        .expect("bool 条件の while は通る");
}

#[test]
fn rejects_non_bool_while_condition() {
    let errs = typecheck("fn f() {\n    while 1 + 1 {\n    }\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("while の条件")));
}

#[test]
fn rejects_break_outside_loop() {
    let errs = typecheck("fn f() {\n    break\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("`break`")));
}

#[test]
fn rejects_continue_outside_loop() {
    let errs = typecheck("fn f() {\n    continue\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("`continue`")));
}

#[test]
fn accepts_break_continue_inside_loop() {
    typecheck("fn f() {\n    loop {\n        if true {\n            break\n        }\n        continue\n    }\n}")
        .expect("ループ内の break/continue は通る");
}

#[test]
fn accepts_for_integer_range() {
    // ループ変数は整数として本体で使え、break/continue も使える。
    typecheck(
        "fn f() {\n    let mut s = 0\n    for i in 0..10 {\n        s = s + i\n        if i == 5 { break }\n    }\n}",
    )
    .expect("整数範囲の for は通る");
}

#[test]
fn for_loop_var_uses_concrete_bound_type() {
    // 具体整数型の境界はループ変数へ伝播する（`i64` 同士の加算が通る）。
    typecheck("fn f(n: i64) {\n    let mut s: i64 = 0\n    for i in 0..n {\n        s = s + i\n    }\n}")
        .expect("境界の i64 がループ変数へ伝播する");
}

#[test]
fn rejects_for_float_range() {
    let errs = typecheck("fn f() {\n    for x in 0.0..5.0 {\n    }\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("整数である必要があります")));
}

#[test]
fn rejects_for_mismatched_bound_types() {
    // 下限 i32（具体型）と上限 i64 は一致しない。
    let errs =
        typecheck("fn f(n: i64) {\n    let a: i32 = 1\n    for k in a..n {\n    }\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("下限と上限の型が一致しません")));
}

#[test]
fn rejects_assigning_to_for_loop_var() {
    // ループ変数は不変束縛。
    let errs = typecheck("fn f() {\n    for i in 0..3 {\n        i = 9\n    }\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("再代入できません")));
}

#[test]
fn for_loop_var_out_of_scope_after_loop() {
    // ループ変数はループ本体の外では未定義（名前解決で弾く）。
    let tokens = lex("fn f() {\n    for i in 0..3 {\n    }\n    let x = i\n}").expect("字句解析");
    let program = parse(&tokens).expect("構文解析");
    assert!(resolve(&program, Default::default()).is_err(), "ループ変数 i はループ外では見えないはず");
}

#[test]
fn accepts_match_guard_with_bool() {
    // ガードが bool なら通る。ペイロード束縛をガードから参照できる。
    typecheck(
        "type Shape = enum {\n    Circle\n    Rect(i32)\n}\nfn f(s: Shape): i32 {\n    match s {\n        Rect(side) if side > 0 -> 1\n        _ -> 0\n    }\n}",
    )
    .expect("bool ガードの match は通る");
}

#[test]
fn rejects_non_bool_match_guard() {
    // ガードは bool でなければならない。
    let errs = typecheck(
        "fn f(n: i32): i32 {\n    match n {\n        _ if n + 1 -> 1\n        _ -> 0\n    }\n}",
    )
    .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("match ガード")));
}

#[test]
fn accepts_numeric_range_pattern() {
    // 数値の範囲パターンは scrutinee の型に適合すれば通る。
    typecheck(
        "fn f(n: i32): i32 {\n    match n {\n        0..10 -> 1\n        10..=20 -> 2\n        _ -> 0\n    }\n}",
    )
    .expect("数値範囲パターンは通る");
}

#[test]
fn rejects_range_pattern_type_mismatch() {
    // 文字列 scrutinee に整数範囲パターンは不可。
    let errs = typecheck(
        "fn f(s: string): i32 {\n    match s {\n        0..10 -> 1\n        _ -> 0\n    }\n}",
    )
    .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("範囲パターンの型")));
}

#[test]
fn rejects_mixed_range_bounds() {
    // 範囲の下限・上限の数値クラスが異なるのは不可。
    let errs = typecheck(
        "fn f(n: i32): i32 {\n    match n {\n        0..10.5 -> 1\n        _ -> 0\n    }\n}",
    )
    .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("同じ数値型")));
}

#[test]
fn example_hello_typechecks() {
    let src = include_str!("../examples/hello.iris");
    typecheck(src).expect("サンプルは型検査を通るはず");
}

#[test]
fn rejects_for_in_non_iterator() {
    // Iterator を実装していない型は `for ... in` で反復できない。
    let errs = typecheck(
        "type P = struct {\n    a: i32\n}\nfn f() {\n    let p = P { a: 1 }\n    for x in p {\n        let y = x\n    }\n}",
    )
    .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("Iterator")));
}

#[test]
fn rejects_generic_struct_inconsistent_fields() {
    // Pair<T> の両フィールドは同じ T。a で T=整数に推論され、b: bool は不一致。
    let errs = typecheck(
        "type Pair<T> = struct {\n    a: T\n    b: T\n}\nfn f() {\n    let p = Pair { a: 1, b: true }\n}",
    )
    .unwrap_err();
    assert!(errs.iter().any(|m| m.contains("型が一致しません")));
}

#[test]
fn array_literal_assigns_to_fixed_array_and_vec() {
    // 同じ配列リテラルが固定長配列 `T[]` にも動的配列 `Vec<T>` にも適合する（型指向）。
    typecheck("fn f() {\n    let a: i32[] = [1, 2, 3]\n    let v: Vec<i32> = [4, 5]\n}")
        .expect("配列リテラルは T[]/Vec<T> の双方へ適合");
}

#[test]
fn array_literal_element_type_mismatch_rejected() {
    let errs = typecheck("fn f() {\n    let a: i32[] = [1, true]\n}").unwrap_err();
    assert!(errs.iter().any(|m| m.contains("一致しません")));
}

#[test]
fn for_over_array_binds_element_type() {
    // 配列を反復するとループ変数は要素型になり、本体で使える。
    typecheck("fn f(): i32 {\n    let a: i32[] = [1, 2, 3]\n    let mut s = 0\n    for x in a {\n        s = s + x\n    }\n    return s\n}")
        .expect("配列の for は要素型を束縛する");
}

#[test]
fn for_over_vec_binds_element_type() {
    typecheck("fn f(): i32 {\n    let v: Vec<i32> = [1, 2]\n    let mut s = 0\n    for x in v {\n        s = s + x\n    }\n    return s\n}")
        .expect("Vec の for は要素型を束縛する");
}

#[test]
fn for_over_non_copy_element_rejected() {
    // 当面、要素が Copy 型でない配列は反復できない。
    let src = "type P = struct { x: i32 }\nfn f() {\n    let a: P[] = [P { x: 1 }]\n    for p in a {\n    }\n}";
    let errs = typecheck(src).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("Copy")));
}
