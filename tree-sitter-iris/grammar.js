module.exports = grammar({
  name: "iris",

  // 空白・タブ・CR は自動で読み飛ばす
  extras: ($) => [/[ \t\r]+/, $.comment],

  conflicts: ($) => [
    [$._stmt, $._expr],
    [$.ref_type, $.array_type],
    [$._expr, $.struct_literal],
    [$.tuple_literal, $.paren_expr],
    [$.method_call_expr, $.field_expr],
    [$._type, $.generic_type],
    [$._pattern, $.enum_pattern],
  ],

  // 演算子の優先順位（低い番号 = 低優先度）
  precedences: (_) => [
    [
      "assign",
      "ternary",
      "or",
      "and",
      "compare",
      "add",
      "mul",
      "unary",
      "call",
      "field",
    ],
  ],

  rules: {
    source_file: ($) => repeat($._item),

    // ---- トップレベル要素 ----

    _item: ($) =>
      choice(
        $.fn_def,
        $.impl_block,
        $.type_def,
        $.extern_fn,
        $.use_decl,
        $.const_decl,
        $.newline,
      ),

    fn_def: ($) =>
      seq(
        optional("pub"),
        "fn",
        field("name", $.identifier),
        $.param_list,
        optional(seq(":", field("return_type", $._type))),
        field("body", $.block),
      ),

    impl_block: ($) =>
      seq(
        "impl",
        field("type_name", $.type_identifier),
        "{",
        repeat(choice($.fn_def, $.newline)),
        "}",
      ),

    type_def: ($) =>
      seq(
        optional("pub"),
        "type",
        field("name", $.type_identifier),
        "=",
        field("body", choice($._type, $.struct_body, $.enum_body)),
        $.newline,
      ),

    struct_body: ($) =>
      seq("struct", "{", repeat(choice($.field_def, $.newline)), "}"),

    enum_body: ($) =>
      seq("enum", "{", repeat(choice($.variant_def, $.newline)), "}"),

    field_def: ($) =>
      seq(field("name", $.identifier), ":", field("type", $._type), $.newline),

    variant_def: ($) =>
      seq(
        field("name", $.type_identifier),
        optional(seq("(", $._type, ")")),
        $.newline,
      ),

    extern_fn: ($) =>
      seq(
        "extern",
        "fn",
        field("name", $.identifier),
        $.param_list,
        optional(seq(":", $._type)),
        $.newline,
      ),

    use_decl: ($) =>
      seq(
        "use",
        field("path", $.use_path),
        $.newline,
      ),

    use_path: ($) => seq($.identifier, repeat(seq("::", $.identifier))),

    const_decl: ($) =>
      seq(
        optional("pub"),
        "const",
        field("name", $.identifier),
        ":",
        field("type", $._type),
        "=",
        field("value", $._expr),
        $.newline,
      ),

    // ---- 型 ----

    _type: ($) =>
      choice(
        $.primitive_type,
        $.type_identifier,
        $.ref_type,
        $.generic_type,
        $.array_type,
        $.option_type,
        $.tuple_type,
      ),

    primitive_type: (_) =>
      choice("i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "string"),

    ref_type: ($) =>
      seq("&", optional("mut"), $._type),

    generic_type: ($) =>
      seq($.type_identifier, "<", $._type, ">"),

    array_type: ($) => seq($._type, "[", "]"),

    option_type: ($) => seq("Option", "<", $._type, ">"),

    tuple_type: ($) => seq("(", $._type, repeat(seq(",", $._type)), ")"),

    // ---- パラメータ ----

    param_list: ($) =>
      seq("(", optional(seq($.param, repeat(seq(",", $.param)))), ")"),

    param: ($) =>
      choice(
        // self / &self / &mut self
        seq(optional(seq("&", optional("mut"))), "self"),
        // name: Type / &name: Type / &mut name: Type
        seq(
          optional(seq("&", optional("mut"))),
          field("name", $.identifier),
          ":",
          field("type", $._type),
        ),
      ),

    // ---- ブロック ----

    block: ($) => seq("{", repeat($._stmt), "}"),

    _stmt: ($) =>
      choice(
        $.let_stmt,
        $.return_stmt,
        $.if_expr,
        $.while_stmt,
        $.loop_stmt,
        $.for_stmt,
        $.break_stmt,
        $.continue_stmt,
        $.assign_stmt,
        $.expr_stmt,
        $.newline,
      ),

    let_stmt: ($) =>
      seq(
        "let",
        optional("mut"),
        field("name", $.identifier),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expr),
        $.newline,
      ),

    return_stmt: ($) => seq("return", optional($._expr), $.newline),

    assign_stmt: ($) =>
      seq(field("target", $._expr), "=", field("value", $._expr), $.newline),

    expr_stmt: ($) => seq($._expr, $.newline),

    break_stmt: ($) => seq("break", $.newline),
    continue_stmt: ($) => seq("continue", $.newline),

    while_stmt: ($) => seq("while", field("cond", $._expr), field("body", $.block)),

    loop_stmt: ($) => seq("loop", field("body", $.block)),

    for_stmt: ($) =>
      seq(
        "for",
        field("var", $.identifier),
        "in",
        field("iter", $._expr),
        field("body", $.block),
      ),

    // ---- 式 ----

    _expr: ($) =>
      choice(
        $.binary_expr,
        $.unary_expr,
        $.call_expr,
        $.method_call_expr,
        $.field_expr,
        $.index_expr,
        $.ternary_expr,
        $.if_expr,
        $.match_expr,
        $.range_expr,
        $.cast_expr,
        $.propagate_expr,
        $.struct_literal,
        $.array_literal,
        $.tuple_literal,
        $.integer_literal,
        $.float_literal,
        $.string_literal,
        $.interp_string_literal,
        $.bool_literal,
        $.identifier,
        $.type_identifier,
        $.paren_expr,
      ),

    binary_expr: ($) =>
      choice(
        prec.left("or", seq($._expr, "||", $._expr)),
        prec.left("and", seq($._expr, "&&", $._expr)),
        prec.left("compare", seq($._expr, choice("==", "!=", "<", "<=", ">", ">="), $._expr)),
        prec.left("add", seq($._expr, choice("+", "-"), $._expr)),
        prec.left("mul", seq($._expr, choice("*", "/", "%"), $._expr)),
      ),

    unary_expr: ($) =>
      prec("unary", seq(choice("!", "-", "&", seq("&", "mut")), $._expr)),

    call_expr: ($) =>
      prec("call", seq(field("func", $._expr), $.arg_list)),

    method_call_expr: ($) =>
      prec("field",
        seq(
          field("receiver", $._expr),
          ".",
          field("method", $.identifier),
          $.arg_list,
        ),
      ),

    field_expr: ($) =>
      prec("field", seq(field("object", $._expr), ".", field("field", $.identifier))),

    index_expr: ($) =>
      prec("field", seq(field("object", $._expr), "[", field("index", $._expr), "]")),

    ternary_expr: ($) =>
      prec.right("ternary",
        seq(field("cond", $._expr), "?", field("then", $._expr), ":", field("else", $._expr)),
      ),

    range_expr: ($) =>
      prec.left("add",
        seq($._expr, choice("..", "..="), $._expr),
      ),

    cast_expr: ($) =>
      prec.left("call", seq($._expr, "as", $._type)),

    propagate_expr: ($) =>
      prec("call", seq($._expr, "!")),

    struct_literal: ($) =>
      seq(
        field("name", $.type_identifier),
        "{",
        optional(seq($.struct_field_init, repeat(seq($.newline, $.struct_field_init)))),
        optional($.newline),
        "}",
      ),

    struct_field_init: ($) =>
      seq(field("name", $.identifier), ":", field("value", $._expr)),

    array_literal: ($) =>
      seq("[", optional(seq($._expr, repeat(seq(",", $._expr)))), "]"),

    tuple_literal: ($) =>
      seq("(", $._expr, repeat(seq(",", $._expr)), ")"),

    if_expr: ($) =>
      seq(
        "if",
        field("cond", $._expr),
        field("then", $.block),
        optional(seq("else", field("else", choice($.block, $.if_expr)))),
      ),

    match_expr: ($) =>
      seq("match", field("value", $._expr), "{", repeat(choice($.match_arm, $.newline)), "}"),

    match_arm: ($) =>
      seq(field("pattern", $._pattern), "->", field("body", $._expr), $.newline),

    _pattern: ($) =>
      choice(
        $.wildcard_pattern,
        $.identifier,
        $.type_identifier,
        $.enum_pattern,
        $.literal_pattern,
      ),

    wildcard_pattern: (_) => "_",

    enum_pattern: ($) =>
      seq($.type_identifier, optional(seq("(", $._pattern, ")"))),

    literal_pattern: ($) =>
      choice($.integer_literal, $.bool_literal, $.string_literal),

    arg_list: ($) =>
      seq("(", optional(seq($._expr, repeat(seq(",", $._expr)))), ")"),

    paren_expr: ($) => seq("(", $._expr, ")"),

    // ---- リテラル ----

    integer_literal: (_) => /[0-9]+/,

    float_literal: (_) => /[0-9]+\.[0-9]*/,

    string_literal: (_) => /"([^"\\]|\\.)*"/,

    interp_string_literal: ($) =>
      seq(
        "`",
        repeat(choice(/[^`{\\]+/, seq("{", $._expr, "}"), /\\./)),
        "`",
      ),

    bool_literal: (_) => choice("true", "false"),

    // ---- 識別子 ----

    // 小文字始まり（変数・関数・フィールド名）
    identifier: (_) => /[a-z_][a-zA-Z0-9_]*/,

    // 大文字始まり（型名・enum バリアント）
    type_identifier: (_) => /[A-Z][a-zA-Z0-9_]*/,

    // ---- コメント・改行 ----

    comment: (_) => token(seq("//", /.*/)),

    newline: (_) => /\n+/,
  },
});
