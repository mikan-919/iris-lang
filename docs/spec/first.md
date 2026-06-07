言語の仕様を確定させる

## 変数宣言
`let name :Type = <expression>`
`LetStatement → "let" IDENT ":" Type "=" Expression`
`LET IDENT COLON IDENT EQUAL EXPRESSION`
シンプルな変数宣言

## 函数宣言
`fn name(x:Type, y:Type):Type -> {<statements>}`
`FuncDef → "fn" IDENT "(" IDENT ":" Type "," IDENT ":" Type ")" ":" Type "->" "{" Statements "}"`
`FN IDENT LPAREN IDENT COLON Type COMMA IDENT COLON Type RPAREN COLON Type ARROW LBRACE Statements RBRACE`
シンプルな函数宣言




## パイプライン函数宣言
`fn name(x:Type, y:Type):Type -> <pipeline>`
`PipelineFuncDef → "fn" IDENT "(" IDENT ":" Type "," IDENT ":" Type ")" ":" Type "->" Pipeline`
`FN IDENT LPAREN IDENT COLON Type COMMA IDENT COLON Type RPAREN COLON Type ARROW PIPELINE`
シンプルなパイプライン函数宣言
## パイプライン（Pipeline）
`Pipeline → Expression ( BackboneOp StepExpression )*`
`PIPELINE → EXPRESSION ( BACKBONE_OP STEP_EXPRESSION )*`
主語（Expression）から始まり、背骨演算子を介して段階的に「ステップ式」を連結する構造。

## ステップ式（StepExpression）
`StepExpression → ImplicitCall | ExplicitCall | AliasedCall | BinaryOpRightHand | FlowStructure`
`STEP_EXPRESSION → IMPLICIT_CALL | EXPLICIT_CALL | ALIASED_CALL | BINARY_OP_RIGHT_HAND | FLOW_STRUCTURE`
現在流れている主体（Subject）をどのように処理するかを記述する、パイプライン専用の特殊な式。

### 1. 暗黙的呼び出し（ImplicitCall）
`ImplicitCall → IDENT "(" ArgList? ")"`
`IMPLICIT_CALL → IDENT LPAREN ARG_LIST? RPAREN`
主語が自動的に第一引数としてバインドされる関数呼び出し。
（例：`text :: trim()` は `trim(text)` と等価）

### 2. 明示的呼び出し（ExplicitCall）
`ExplicitCall → IDENT "(" PlaceholderArgList ")"`
`EXPLICIT_CALL → IDENT LPAREN PLACEHOLDER_ARG_LIST RPAREN`
プレースホルダ `$` を用いて、主語を第一引数以外の任意の位置に代入する関数呼び出し。
（例：`text :: includes(baseText, $)` は `includes(baseText, text)` と等価）

### 3. エイリアス付き呼び出し（AliasedCall）
`AliasedCall → IDENT "$" IDENT "(" Expression ")"`
`ALIASED_CALL → IDENT DOLLAR IDENT LPAREN EXPRESSION RPAREN`
主語に一時的な名前（エイリアス）を与え、その名前を用いて後続の式を評価する呼び出し。
（例：`foo$x(x + bar(x))` は、主語を `x` とした上で `foo(x + bar(x))` として評価）

### 4. 二項演算右辺（BinaryOpRightHand）
`BinaryOpRightHand → BINARY_OP Expression`
`BINARY_OP_RIGHT_HAND → BINARY_OP EXPRESSION`
主語を左辺とし、指定された演算子と右辺の式を評価する。
（例：`input :: * factor` は `input * factor` と等価）

---

## 補助ルールと新規トークン

### 補助ルール
`ArgList → Expression ( "," Expression )*`
`PlaceholderArg → Expression | "$"`
`PlaceholderArgList → PlaceholderArg ( "," PlaceholderArg )*`

### 新規トークン
*   `DOLLAR` : `$`（エイリアスおよびプレースホルダに使用）
## 制御・分岐フロー（FlowStructure）
`FlowStructure → MatchStructure | JoinStructure | IfStructure | WhileStructure`
`FLOW_STRUCTURE → MATCH_STRUCTURE | JOIN_STRUCTURE | IF_STRUCTURE | WHILE_STRUCTURE`
値を評価し、パイプラインを分岐・合流、あるいは反復させるための構造。

### 1. マッチ分岐（MatchStructure）
`MatchStructure → "match" "(" MatchBranch* ")"`
`MATCH_STRUCTURE → "match" LPAREN MATCH_BRANCH* RPAREN`
`MatchBranch → Pattern "->` Pipeline`
パターンにマッチした枝のパイプラインを評価する。

### 2. ジョイン・タプル（JoinStructure）
`JoinStructure → "(" JoinBranch ( JoinBranch )+ ")"`
`JOIN_STRUCTURE → LPAREN JOIN_BRANCH JOIN_BRANCH+ RPAREN`
`JoinBranch → "|" ( BackboneOp | ) Pipeline`
複数のパイプラインを並行評価（または順次評価）し、タプルに合流させる。
（`| ::` で始まれば親の主体を引き継ぎ、値を直接置けば新しい主体から開始する）

### 3. 条件分岐（IfStructure）
`IfStructure → ":if" Expression ":then" Expression ( ":else" Expression )?`
`IF_STRUCTURE → COLON_IF EXPRESSION COLON_THEN EXPRESSION ( COLON_ELSE EXPRESSION )?`
条件式の真偽によって評価する式を切り替える。

### 4. ループ（WhileStructure）
`WhileStructure → ":while" Expression ":then" Expression`
`WHILE_STRUCTURE → COLON_WHILE EXPRESSION COLON_THEN EXPRESSION`
条件が真である限り、主体を消費または更新しながらループを反復実行する。
## ラムダ式と首なしパイプライン
`LambdaExpr → "(" LambdaParamList "->" Expression ")"`
`LAMBDA_EXPR → LPAREN LAMBDA_PARAM_LIST ARROW EXPRESSION RPAREN`
引数を受け取り、指定された式を評価する匿名関数（クロージャ）を生成する。

`HeadlessPipeline → "(" PipelineStep+ ")"`
`HEADLESS_PIPELINE → LPAREN PIPELINE_STEP+ RPAREN`
暗黙の主体（引数1つ）を受け取り、指定されたパイプラインに流す、無名のフローオブジェクトを生成する。
（例：`( :: trim() :: lower() )`）

---

## 補助ルール

### 引数リスト
`LambdaParamList → ( LambdaParam ( "," LambdaParam )* )?`
`LambdaParam → IDENT ( ":" Type )?`
ラムダ式の引数定義。型アノテーションは省略可能。
## モジュールと名前空間

### 1. インポート（UseStatement）
`UseStatement → "use" Path ( "." "*" )?`
`USE_STATEMENT → "use" PATH ( DOT STAR )?`
外部モジュール、あるいはその中身を現在のファイルスコープ（名前解決の探索範囲）に導入する。
（例：`use std.*`, `use std.math.clamp`）

### 2. パス（Path）
`Path → IDENTOrTrait ( "." IDENTOrTrait )*`
`PATH → IDENT_OR_TRAIT ( DOT IDENT_OR_TRAIT )*`
モジュール、Trait、あるいは関数を指し示すためのドット連結された名前階層。

---

## 補助ルールと新規トークン

### 補助ルール
`IDENTOrTrait → IDENT | TRAIT_IDENT`
`IDENT_OR_TRAIT → IDENT | TRAIT_IDENT`
通常の識別子（関数名や型名など）、または `#` から始まるTrait識別子。

### 新規トークン
*   `TRAIT_IDENT` : `#` から始まる識別子（例：`#Text`, `#String`）
*   `DOT` : `.`（モジュールやTraitの階層を繋ぐパス区切り）
*   `STAR` : `*`（モジュール内の全インポートに使用）
## Trait と Impl（能力宣言と実装）

### 1. Trait 定義（TraitDef）
`TraitDef → "trait" IDENT ( ":" IDENT )? "{" TraitMember* "}"`
`TRAIT_DEF → "trait" IDENT ( COLON IDENT )? LBRACE TRAIT_MEMBER* RBRACE`
能力契約を定義する。他のTraitからの継承（ `:` 指定）や、デフォルト実装をサポートする。

`TraitMember → MethodSignature | FuncDef | PipelineFuncDef`
`TRAIT_MEMBER → METHOD_SIGNATURE | FUNC_DEF | PIPELINE_FUNC_DEF`

### 2. メソッドシグネチャ（MethodSignature）
`MethodSignature → "fn" IDENT "(" ParamList? ")" ":" Type`
`METHOD_SIGNATURE → "fn" IDENT LPAREN PARAM_LIST? RPAREN COLON TYPE`
実装を持たない、関数名と引数・戻り値型のみの契約宣言。

### 3. Impl 定義（ImplDef）
`ImplDef → "impl" IDENT "for" Type "{" ImplMember* "}"`
`IMPL_DEF → "impl" IDENT "for" TYPE LBRACE IMPL_MEMBER* RBRACE`
特定の具体的な型（Type）に対して、指定したTraitの実装を提供する。

`ImplMember → FuncDef | PipelineFuncDef`
`IMPL_MEMBER → FUNC_DEF | PIPELINE_FUNC_DEF`
## Comptimeディレクティブ と ジェネリクス

### 1. Comptimeディレクティブ（ComptimeDirective）
`ComptimeDirective → "!" IDENT "(" ArgList? ")"`
`COMPTIME_DIRECTIVE → BANG IDENT LPAREN ARG_LIST? RPAREN`
コンパイル時に評価・検査される命令やアノテーション。
（例：`!require(Readable)`, `!derive(Debug)`）

### 2. ジェネリクス能力制約（GenericParamList）
`GenericParamList → GenericParam ( "," GenericParam )*`
`GENERIC_PARAM_LIST → GENERIC_PARAM ( COMMA GENERIC_PARAM )*`

`GenericParam → IDENT ( ":" TraitBound )?`
`GENERIC_PARAM → IDENT ( COLON TRAIT_BOUND )?`

`TraitBound → IDENT ( "&&" IDENT )*`
`TRAIT_BOUND → IDENT ( AND_AND IDENT )*`
型パラメータに要求されるTrait能力。`&&` を用いて複数の能力（Trait）を合成・要求できる。
（例：`T: Text && Serializable`）

---

## 既存ルールの拡張

### 関数定義（FuncDef / PipelineFuncDef）
型パラメータと能力制約を受け入れられるよう、関数名の直後に `<GenericParamList>` を挿入できるように拡張します。

*   `FuncDef → "fn" IDENT ( "<" GenericParamList ">" )? "(" ParamList? ")" ":" Type "->" "{" Statements "}"`
*   `PipelineFuncDef → "fn" IDENT ( "<" GenericParamList ">" )? "(" ParamList? ")" ":" Type "->" Pipeline`

---

## 新規トークン
*   `BANG` : `!` (Comptimeディレクティブの開始シンボル)
*   `AND_AND` : `&&` (能力合成の論理積演算子)
*   `LT` : `<` (ジェネリクス開始)
*   `GT` : `>` (ジェネリクス終了)
