## Hello World コンパイル時の関数呼び出しグラフ

`examples/hello.iris` をコンパイルする際の実行パス。未定義・未呼び出しの関数は省略。

```mermaid
callGraph
    main["main()\nデフォルト動作: AST表示"]
    dump_ast["dump_ast()\nASTを標準出力へ表示"]
    compile["compile()\n全パイプラインを直列実行"]
    analyze["analyze()\n解析パイプラインの起点\n(全段を呼び出し)]

    lexer["lexer::lex()\nnom 8 + nom_locate\nトークン列を生成"]
    parser["parser::parse()\nnom による構文解析\nASTを生成"]

    resolve["sema::resolve()\nスコープ構築・使用→定義対応"]
    typeck["sema::typeck::check()\n式に型を付与・互換性検査"]

    own_check["sema::ownership::check()\n所有権三層検査"]
    check_cycles["typegraph::check_cycles()\n型レベルの所有グラフ＋\n循環検出(DFS)"]
    flow_check["flow::check_functions()\n全関数の値レベルの\nムーブ/借用グラフ検査"]
    borrows_check["borrows::check_borrows()\n&mut排他/&複数可の\n借用競合検査"]

    emit["codegen::emit_module()\nLLVM IRテキスト生成\nのオーケストレーター"]
    struct_reg["StructReg::build()\nstructレイアウト・enum\nレイアウト・別名を収集"]
    emit_fn["FnCodegen::emit_function()\n関数1つ分のLLVM IRを\n生成"]
    gen_stmt["FnCodegen::gen_stmt()\n文をLLVM IRに\n変換"]
    gen_expr["FnCodegen::gen_expr()\n式をLLVM IRに\n変換"]

    llvm_ty["llvm_ty()\n内部型TyをLLVM型へ\n変換"]
    subst_ty["subst_ty()\n型パラメータを具体型へ\n置換(単相化)"]
    mono_symbol["mono_symbol()\nジェネリックの単相化\n記号を生成"]

    flow_run["Flow::run_function()\n関数1つ分の所有権\nフロー解析"]
    flow_walk["Flow::walk_stmts/exprs()\nASTを再帰走査し\nムーブ/借用を検査"]

    main --> dump_ast
    dump_ast --> compile
    compile --> analyze

    analyze --> lexer
    analyze --> parser
    analyze --> resolve
    analyze --> typeck
    analyze --> own_check
    analyze --> emit

    own_check --> check_cycles
    own_check --> flow_check
    own_check --> borrows_check

    emit --> struct_reg
    emit --> emit_fn

    emit_fn --> gen_stmt
    emit_fn --> gen_expr
    gen_stmt --> gen_expr

    gen_expr --> llvm_ty
    gen_expr --> subst_ty
    gen_expr --> mono_symbol

    flow_check --> flow_run
    flow_run --> flow_walk
```
