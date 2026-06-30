; キーワード
[
  "fn" "let" "const" "return" "if" "else" "while" "loop"
  "for" "in" "break" "continue" "pub" "mut" "type" "struct"
  "enum" "extern" "match" "use" "as" "impl"
] @keyword

; 型プリミティブ
(primitive_type) @type.builtin

; ユーザ定義型（大文字始まり）
(type_identifier) @type

; 関数定義名
(fn_def name: (identifier)) @function

; 関数呼び出し
(call_expr func: (identifier)) @function.call
(method_call_expr method: (identifier)) @function.method

; 変数・フィールド
(identifier) @variable
(field_expr field: (identifier)) @property
(struct_field_init name: (identifier)) @property

; リテラル
(integer_literal) @number
(float_literal) @number.float
(string_literal) @string
(interp_string_literal) @string.special
(bool_literal) @boolean

; 演算子
["+" "-" "*" "/" "%" "==" "!=" "<" "<=" ">" ">=" "&&" "||" "!" "?" "&" "=" "->"] @operator
[".." "..="] @operator

; コメント
(comment) @comment

; self パラメータ
((identifier) @variable.builtin
 (#eq? @variable.builtin "self"))
