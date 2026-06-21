//! 型レベルの所有権グラフと循環（無限サイズ型）検出。
//!
//! ノードはユーザー定義型（`type` で定義した名前）。辺 `A → B` は「A が B の値を
//! インラインに（＝サイズに含まれる形で）所有する」関係。サイクルは所有の循環＝
//! 無限サイズ型であり、仕様の「所有権グラフは DAG でなければならない」に反する。
//!
//! 所有辺を作らないもの:
//! - 参照 `&T` … 非所有
//! - `Box<T>` / `Vec<T>` / `Map<..>` / `Set<T>` … ヒープ間接（ポインタ越し）
//!
//! 所有辺を作るもの（インライン格納）:
//! - 名前付き型 `B`（B がユーザー型なら辺）
//! - `Option<T>` / `Result<T, E>` / タプル / 固定長配列 `T[]` … 中身をインライン保持
//! - 別名 `type A = T` … サイズ上は透過

use std::collections::{HashMap, HashSet};

use crate::ast::{Item, Program, Type, TypeDefBody};
use crate::span::Span;

use super::OwnershipError;

/// ヒープ間接（所有辺を断ち切る）組み込み型。
const HEAP_INDIRECT: &[&str] = &["Box", "Vec", "Map", "Set"];

pub fn check_cycles(program: &Program, errors: &mut Vec<OwnershipError>) {
    // ユーザー定義型の名前と宣言位置。
    let mut spans: HashMap<&str, Span> = HashMap::new();
    for item in &program.items {
        if let Item::TypeDef(t) = item {
            spans.insert(&t.name, t.name_span);
        }
    }

    // 隣接リスト（インライン所有辺）。
    let mut adj: HashMap<&str, Vec<String>> = HashMap::new();
    for item in &program.items {
        if let Item::TypeDef(t) = item {
            let mut targets = Vec::new();
            match &t.body {
                TypeDefBody::Alias(ty) => collect_owned(ty, &spans, &mut targets),
                TypeDefBody::Struct(fields) => {
                    for f in fields {
                        collect_owned(&f.ty, &spans, &mut targets);
                    }
                }
                TypeDefBody::Enum(variants) => {
                    for v in variants {
                        if let Some(ty) = &v.payload {
                            collect_owned(ty, &spans, &mut targets);
                        }
                    }
                }
            }
            adj.insert(&t.name, targets);
        }
    }

    // DFS でサイクルを検出する。サイクルに含まれるノードは一度だけ報告する。
    let mut state: HashMap<String, Visit> = HashMap::new();
    let mut reported: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();
    let mut names: Vec<&str> = adj.keys().copied().collect();
    names.sort_unstable();
    for &start in &names {
        dfs(start, &adj, &spans, &mut state, &mut stack, &mut reported, errors);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Visit {
    Active,
    Done,
}

#[allow(clippy::too_many_arguments)]
fn dfs(
    node: &str,
    adj: &HashMap<&str, Vec<String>>,
    spans: &HashMap<&str, Span>,
    state: &mut HashMap<String, Visit>,
    stack: &mut Vec<String>,
    reported: &mut HashSet<String>,
    errors: &mut Vec<OwnershipError>,
) {
    if let Some(&v) = state.get(node) {
        if v == Visit::Active {
            // 後退辺を検出。スタック上の node から現在までがサイクル。
            report_cycle(node, stack, spans, reported, errors);
        }
        return;
    }
    state.insert(node.to_string(), Visit::Active);
    stack.push(node.to_string());
    if let Some(targets) = adj.get(node) {
        for t in targets {
            dfs(t, adj, spans, state, stack, reported, errors);
        }
    }
    stack.pop();
    state.insert(node.to_string(), Visit::Done);
}

fn report_cycle(
    node: &str,
    stack: &[String],
    spans: &HashMap<&str, Span>,
    reported: &mut HashSet<String>,
    errors: &mut Vec<OwnershipError>,
) {
    // スタックから node 以降を取り出してサイクル経路にする。
    let start = stack.iter().position(|n| n == node).unwrap_or(0);
    let cycle = &stack[start..];
    // 既に報告済みのノードを含むなら重複報告しない。
    if cycle.iter().any(|n| reported.contains(n)) {
        return;
    }
    for n in cycle {
        reported.insert(n.clone());
    }
    let path = if cycle.len() == 1 {
        format!("`{}` → `{}`", cycle[0], cycle[0])
    } else {
        let mut p: Vec<&str> = cycle.iter().map(String::as_str).collect();
        p.push(node); // 経路を閉じる
        p.iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(" → ")
    };
    let span = spans.get(node).copied().unwrap_or(Span::new(0, 0));
    errors.push(OwnershipError::new(
        span,
        format!("所有が循環しています（無限サイズの型）: {path}。`&` か `Box`/`Vec` で間接化してください"),
    ));
}

/// 型 `ty` がインラインに所有するユーザー定義型の名前を集める。
fn collect_owned(ty: &Type, defs: &HashMap<&str, Span>, out: &mut Vec<String>) {
    match ty {
        Type::Named { name, args, .. } => {
            if HEAP_INDIRECT.contains(&name.as_str()) {
                // ヒープ間接。中身はサイズに含まれないので辺を作らない。
                return;
            }
            // ユーザー定義型なら所有辺。
            if defs.contains_key(name.as_str()) {
                out.push(name.clone());
            }
            // Option/Result/その他ジェネリックの型引数はインライン格納とみなす。
            for a in args {
                collect_owned(a, defs, out);
            }
        }
        // 参照は非所有。
        Type::Ref { .. } => {}
        // 固定長配列・タプルはインライン。
        Type::Array { inner, .. } => collect_owned(inner, defs, out),
        Type::Tuple { elems, .. } => {
            for e in elems {
                collect_owned(e, defs, out);
            }
        }
        // 匿名境界（ジェネリック）は所有辺を作らない。
        Type::Bound { .. } => {}
    }
}
