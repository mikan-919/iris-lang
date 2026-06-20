//! 値レベルの前進フロー解析。ムーブ検査と、借用グラフ（provenance）による
//! ライフタイム検査を行う。
//!
//! ## ムーブ
//! 値を関数引数・`let`・`return`・構造体リテラルのフィールドへ「値として」渡すと
//! ムーブする。`Copy` 型はムーブしない。`&x`/`&mut x` とメソッド受け手は借用。
//! ムーブ済みの値の使用・借用はエラー。再代入で再初期化。`if`/三項の分岐は
//! ムーブ集合を保守的に合流する。
//!
//! ## ライフタイム（借用グラフ）
//! 参照値の「出所」を、関数が所有する束縛（ローカル・値渡し引数・参照引数の格納
//! スロット）への到達可能性として求める（`prov`）。グラフの辺は `let r = &x` で
//! `r → x`、`let r = some_ref` で provenance の伝播として張られる。
//!
//! - **ダングリング返却**: 参照を返すとき、その出所に関数所有の束縛が含まれると、
//!   その束縛より長生きするためエラー。参照引数経由（呼び出し側所有）は安全。
//! - **指す先のムーブ**: ある束縛 `x` がムーブされると、出所に `x` を含む参照は
//!   無効化され、以降の使用はエラー。

use std::collections::{HashMap, HashSet};

use crate::ast::{Block, Else, Expr, ExprKind, Function, Item, Program, Stmt, UnaryOp};
use crate::sema::resolve::{DefId, DefKind, Resolution};
use crate::sema::ty::{FLOAT_TYPES, INT_TYPES, Ty};
use crate::sema::typeck::TypeInfo;
use crate::span::Span;

use super::OwnershipError;

pub fn check_functions(
    program: &Program,
    res: &Resolution,
    type_info: &TypeInfo,
    errors: &mut Vec<OwnershipError>,
) {
    // 別名 `type X = T`（Copy 判定のため）。
    let mut aliases = HashMap::new();
    for item in &program.items {
        if let Item::TypeDef(t) = item
            && let crate::ast::TypeDefBody::Alias(ty) = &t.body
        {
            aliases.insert(t.name.clone(), Ty::from_ast(ty));
        }
    }

    // 束縛 DefId → 型（使用箇所の型から復元）。参照引数の判定などに使う。
    let mut def_type: HashMap<DefId, Ty> = HashMap::new();
    for (span, &id) in &res.uses {
        if let Some(ty) = type_info.expr_types.get(span) {
            def_type.entry(id).or_insert_with(|| ty.clone());
        }
    }
    // 定義位置 span → DefId（`let`・引数の束縛先を引く）。
    let def_spans: HashMap<Span, DefId> = res
        .defs
        .iter()
        .enumerate()
        .map(|(id, d)| (d.span, id))
        .collect();

    let mut a = Flow {
        res,
        types: &type_info.expr_types,
        aliases,
        def_type,
        def_spans,
        prov: HashMap::new(),
        errors,
    };
    for item in &program.items {
        if let Item::Function(f) = item {
            a.run_function(f);
        }
    }
}

/// フロー状態（分岐で複製・合流する）。
#[derive(Clone, Default)]
struct State {
    /// ムーブ済みの束縛 → ムーブ位置。
    moved: HashMap<DefId, Span>,
    /// 指す先がムーブされ無効化された参照束縛 → (無効化位置, 参照先名)。
    dangling: HashMap<DefId, (Span, String)>,
}

struct Flow<'a> {
    res: &'a Resolution,
    types: &'a HashMap<Span, Ty>,
    aliases: HashMap<String, Ty>,
    def_type: HashMap<DefId, Ty>,
    def_spans: HashMap<Span, DefId>,
    /// 参照束縛 DefId → その出所（関数所有の束縛集合）。借用グラフ。
    prov: HashMap<DefId, HashSet<DefId>>,
    errors: &'a mut Vec<OwnershipError>,
}

impl Flow<'_> {
    fn run_function(&mut self, f: &Function) {
        let ret_is_ref = matches!(f.ret.as_ref().map(Ty::from_ast), Some(Ty::Ref { .. }));
        let mut st = State::default();
        self.visit_block(&f.body, &mut st, ret_is_ref);
    }

    fn visit_block(&mut self, block: &Block, st: &mut State, ret_is_ref: bool) {
        for stmt in &block.stmts {
            self.visit_stmt(stmt, st, ret_is_ref);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt, st: &mut State, ret_is_ref: bool) {
        match stmt {
            Stmt::Let { value, span, .. } => {
                self.visit_expr(value, st);
                if let Some(&id) = self.def_spans.get(span) {
                    // 束縛先を再初期化する（ループ反復で同じ DefId が再束縛される）。
                    st.moved.remove(&id);
                    st.dangling.remove(&id);
                    // この let が参照型なら provenance を記録する。
                    if self.is_ref_def(id) {
                        let p = self.prov_of(value);
                        self.prov.insert(id, p);
                    }
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.visit_expr(v, st);
                    if ret_is_ref || matches!(self.types.get(&v.span), Some(Ty::Ref { .. })) {
                        self.check_dangling_return(v);
                    }
                }
            }
            Stmt::Assign { target, value, .. } => {
                self.visit_expr(value, st);
                if let ExprKind::Ident(_) = &target.kind {
                    if let Some(&id) = self.res.uses.get(&target.span) {
                        st.moved.remove(&id);
                        st.dangling.remove(&id);
                        // 参照への再代入は provenance を更新する。
                        if self.is_ref_def(id) {
                            let p = self.prov_of(value);
                            self.prov.insert(id, p);
                        }
                    }
                } else {
                    self.use_place(target, st);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.visit_expr(cond, st);
                self.visit_loop_body(body, st, ret_is_ref);
            }
            Stmt::Loop { body, .. } => {
                self.visit_loop_body(body, st, ret_is_ref);
            }
            // break / continue は制御フローのみ。ムーブ集合は保守的に直線扱いで足りる。
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Expr(e) => self.visit_expr(e, st),
        }
    }

    /// ループ本体を解析する。反復をまたぐ use-after-move を検出するため、まず本体で
    /// 生じるムーブを「静かに」（エラーを捨てて）収集して入口状態に合流させ、その状態
    /// から本パスを 1 回走らせる。これにより末尾でムーブした値を次の反復の先頭で使う
    /// パターンを検出できる。ムーブ集合は単調なので 2 パスで安定する。本体は 0 回
    /// 実行されうるため、ループ後の状態は元の入口と合流する（保守的）。
    fn visit_loop_body(&mut self, body: &Block, st: &mut State, ret_is_ref: bool) {
        let mut probe = st.clone();
        self.visit_block_quiet(body, &mut probe);
        let mut entry = merge(st.clone(), probe);
        self.visit_block(body, &mut entry, ret_is_ref);
        *st = merge(st.clone(), entry);
    }

    /// エラーを記録せずにブロックを解析する（ループの先行パス用）。provenance など
    /// 永続的な副作用は残るが冪等なので問題ない。
    fn visit_block_quiet(&mut self, block: &Block, st: &mut State) {
        let stash = std::mem::take(self.errors);
        self.visit_block(block, st, false);
        *self.errors = stash;
    }

    fn visit_expr(&mut self, expr: &Expr, st: &mut State) {
        match &expr.kind {
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) | ExprKind::Bool(_) => {}
            ExprKind::Ident(name) => self.use_var(name, expr.span, st, true),
            ExprKind::Unary { op, expr: inner } => match op {
                UnaryOp::Ref | UnaryOp::RefMut => self.use_place(inner, st),
                UnaryOp::Neg => self.visit_expr(inner, st),
            },
            ExprKind::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs, st);
                self.visit_expr(rhs, st);
            }
            ExprKind::Call { callee, args } => {
                match &callee.kind {
                    ExprKind::Ident(_) if self.is_callable(callee) => {}
                    ExprKind::Member { object, .. } => self.use_place(object, st),
                    _ => self.visit_expr(callee, st),
                }
                for arg in args {
                    self.visit_expr(arg, st);
                }
            }
            ExprKind::Member { object, .. } => self.use_place(object, st),
            ExprKind::Ternary {
                cond,
                then,
                otherwise,
            } => {
                self.visit_expr(cond, st);
                let mut a = st.clone();
                self.visit_expr(then, &mut a);
                let mut b = st.clone();
                self.visit_expr(otherwise, &mut b);
                *st = merge(a, b);
            }
            ExprKind::Try(inner) => self.visit_expr(inner, st),
            ExprKind::StructLit { fields, .. } => {
                for f in fields {
                    self.visit_expr(&f.value, st);
                }
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.visit_expr(cond, st);
                let mut a = st.clone();
                self.visit_block(then, &mut a, false);
                let mut b = st.clone();
                if let Some(els) = otherwise {
                    match els.as_ref() {
                        Else::If(e) => self.visit_expr(e, &mut b),
                        Else::Block(blk) => self.visit_block(blk, &mut b, false),
                    }
                }
                *st = merge(a, b);
            }
        }
    }

    /// 借用・読みとして式を訪問する（ムーブしない）。
    fn use_place(&mut self, place: &Expr, st: &mut State) {
        match &place.kind {
            ExprKind::Ident(name) => self.use_var(name, place.span, st, false),
            ExprKind::Member { object, .. } => self.use_place(object, st),
            _ => self.visit_expr(place, st),
        }
    }

    fn use_var(&mut self, name: &str, span: Span, st: &mut State, is_move: bool) {
        let Some(&id) = self.res.uses.get(&span) else {
            return;
        };
        if !matches!(self.res.defs[id].kind, DefKind::Local | DefKind::Param) {
            return;
        }
        // 指す先がムーブされて無効化された参照の使用。
        if let Some((inval_span, referent)) = st.dangling.get(&id).cloned() {
            self.errors.push(OwnershipError::with_secondary(
                span,
                format!("ムーブされた値 `{referent}` を指す参照 `{name}` を使用しています"),
                inval_span,
                format!("`{referent}` はここでムーブされました"),
            ));
            return;
        }
        // ムーブ済みの使用・借用。
        if let Some(&move_span) = st.moved.get(&id) {
            let verb = if is_move { "使用" } else { "借用" };
            self.errors.push(OwnershipError::with_secondary(
                span,
                format!("ムーブ済みの値 `{name}` を{verb}しています"),
                move_span,
                "ここでムーブされました".to_string(),
            ));
            return;
        }
        if is_move {
            let ty = self.types.get(&span).cloned().unwrap_or(Ty::Infer);
            if !is_copy(&ty, &self.aliases) {
                st.moved.insert(id, span);
                self.invalidate_refs_to(id, span, st);
            }
        }
    }

    /// `id` がムーブされたとき、出所に `id` を含む参照束縛を無効化する。
    fn invalidate_refs_to(&self, id: DefId, span: Span, st: &mut State) {
        let referent = self.res.defs[id].name.clone();
        for (&r, sources) in &self.prov {
            if sources.contains(&id) && !st.dangling.contains_key(&r) {
                st.dangling.insert(r, (span, referent.clone()));
            }
        }
    }

    /// 参照を返すとき、出所に関数所有の束縛が含まれればダングリング。
    fn check_dangling_return(&mut self, value: &Expr) {
        let sources = self.prov_of(value);
        if let Some(&referent) = sources.iter().next() {
            let def = &self.res.defs[referent];
            self.errors.push(OwnershipError::with_secondary(
                value.span,
                format!(
                    "ローカルな値 `{}` を指す参照を返しています（`{}` より長く生きられません）",
                    def.name, def.name
                ),
                def.span,
                format!("`{}` はこの関数の中で所有されています", def.name),
            ));
        }
    }

    /// 参照式の出所（関数所有の束縛集合）をグラフ上の到達可能性として求める。
    fn prov_of(&self, expr: &Expr) -> HashSet<DefId> {
        match &expr.kind {
            // `&place` / `&mut place` は place の基底束縛（関数所有スロット）を指す。
            ExprKind::Unary {
                op: UnaryOp::Ref | UnaryOp::RefMut,
                expr: inner,
            } => base_owned(inner, self.res).into_iter().collect(),
            ExprKind::Ident(_) => {
                let Some(&id) = self.res.uses.get(&expr.span) else {
                    return HashSet::new();
                };
                // 参照引数は呼び出し側所有 → 出所なし（返却して安全）。
                if self.res.defs[id].kind == DefKind::Param && self.is_ref_def(id) {
                    return HashSet::new();
                }
                self.prov.get(&id).cloned().unwrap_or_default()
            }
            // 参照を返す関数呼び出しは、参照引数の出所を引き継ぐ（ライフタイム省略）。
            ExprKind::Call { args, .. } => {
                let mut s = HashSet::new();
                for a in args {
                    s.extend(self.prov_of(a));
                }
                s
            }
            ExprKind::Ternary {
                then, otherwise, ..
            } => {
                let mut s = self.prov_of(then);
                s.extend(self.prov_of(otherwise));
                s
            }
            ExprKind::Try(inner) => self.prov_of(inner),
            ExprKind::Member { object, .. } => self.prov_of(object),
            _ => HashSet::new(),
        }
    }

    fn is_callable(&self, callee: &Expr) -> bool {
        self.res.uses.get(&callee.span).is_some_and(|&id| {
            matches!(self.res.defs[id].kind, DefKind::Function | DefKind::Builtin)
        })
    }

    /// 束縛が参照型か。
    fn is_ref_def(&self, id: DefId) -> bool {
        matches!(self.def_type.get(&id), Some(Ty::Ref { .. }))
    }
}

/// `place` の基底となる関数所有の束縛（Ident/Member の根）を返す。
fn base_owned(place: &Expr, res: &Resolution) -> Option<DefId> {
    match &place.kind {
        ExprKind::Ident(_) => {
            let id = *res.uses.get(&place.span)?;
            matches!(res.defs[id].kind, DefKind::Local | DefKind::Param).then_some(id)
        }
        ExprKind::Member { object, .. } => base_owned(object, res),
        _ => None,
    }
}

/// 2 つのフロー状態を保守的に合流する。
fn merge(mut a: State, b: State) -> State {
    for (k, v) in b.moved {
        a.moved.entry(k).or_insert(v);
    }
    for (k, v) in b.dangling {
        a.dangling.entry(k).or_insert(v);
    }
    a
}

/// 型が `Copy`（ムーブせずコピー）か。別名はもとの型で判定。不明・エラーは Copy 扱い。
fn is_copy(ty: &Ty, aliases: &HashMap<String, Ty>) -> bool {
    match ty {
        Ty::IntLit | Ty::FloatLit | Ty::Infer | Ty::Error => true,
        Ty::Ref { mutable, .. } => !mutable,
        Ty::Tuple(elems) => elems.iter().all(|e| is_copy(e, aliases)),
        Ty::Array(_) => false,
        Ty::Named { name, args } if args.is_empty() => {
            if INT_TYPES.contains(&name.as_str())
                || FLOAT_TYPES.contains(&name.as_str())
                || name == "bool"
                || name == "char"
            {
                true
            } else if let Some(target) = aliases.get(name) {
                is_copy(target, aliases)
            } else {
                false
            }
        }
        Ty::Named { .. } => false,
    }
}
