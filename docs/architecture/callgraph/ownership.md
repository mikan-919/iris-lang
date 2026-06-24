# 所有権解析コールグラフ

`check_ownership()` から実際に呼ばれる関数のみ。深さ 5 以下、20 ノード以下。

```mermaid
flowchart TD
    check_ownership["check_ownership()\n三層を直列に呼ぶ"]

    subgraph 型グラフ層
        check_cycles["typegraph::check_cycles()\n所有辺グラフを構築しDFSで循環検出"]
        collect_owned["collect_owned()\n型からインライン所有辺を収集"]
        dfs["dfs()\n再帰DFSで閉路を検出"]
    end

    subgraph ムーブフロー層
        check_functions["flow::check_functions()\n全関数を走査"]
        run_function["Flow::run_function()\n1関数のフロー解析"]
        walk_stmts["Flow::walk_stmts()\n文を順に処理しムーブ状態更新"]
        walk_expr["Flow::walk_expr()\nuse-after-move・ダングリング検出"]
    end

    subgraph 借用競合層
        check_borrows["borrows::check_borrows()\n全関数の借用競合を検査"]
        borrow_of["borrow_of()\n&/&mut の借用元を特定"]
    end

    check_ownership --> check_cycles
    check_ownership --> check_functions
    check_ownership --> check_borrows

    check_cycles --> collect_owned
    check_cycles --> dfs

    check_functions --> run_function
    run_function --> walk_stmts
    walk_stmts --> walk_expr

    check_borrows --> borrow_of
```

## 各ノードの責務

| ノード | 責務 |
|---|---|
| `check_ownership()` | `typegraph`・`flow`・`borrows` を直列に呼び、エラーを集約する |
| `check_cycles()` | 型定義の所有辺グラフを構築し DFS で循環（無限サイズ型）を検出する |
| `collect_owned()` | 型 `Type` から `&T`/`Box`/`Vec` を除いた所有辺を収集する |
| `dfs()` | 有向グラフを再帰 DFS し、灰色ノードへの後退辺を閉路として報告する |
| `check_functions()` | 全 Function を走査し `Flow::run_function()` を呼ぶ |
| `Flow::run_function()` | 関数の State（所有フラグ集合）を初期化して `walk_stmts()` を呼ぶ |
| `Flow::walk_stmts()` | 文を前進走査し State を更新する。if/match 分岐では各腕を merge する |
| `Flow::walk_expr()` | 識別子の use-after-move・ダングリング返却を State を参照して検出する |
| `check_borrows()` | 全 Function を走査し `&`/`&mut` の競合（排他性違反）を検出する |
| `borrow_of()` | `&e`/`&mut e` 式から借用元の DefId と可変性を返す |
