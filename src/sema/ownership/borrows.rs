//! 借用競合チェック（エイリアス規則）。
//!
//! 同一の場所（束縛）に対し、可変借用 `&mut` は排他、共有借用 `&` は複数可。
//! すなわち `&mut` が生きている間は他の借用（`&`/`&mut`）を持てない。
//!
//! 借用の生存期間は **スコープベース**（保守的）: `let r = &x` で作られた借用は
//! その束縛を宣言したブロックの終わりまで生きるとみなす。式の中で一時的に作られる
//! 借用（`f(&mut x, &x)` など）はその文の評価中だけ生きるとみなす。
//!
//! この近似は健全（実際の競合は見逃さない）だが、NLL のような最後の使用に基づく
//! 精密な生存期間ではないため、保守的に多めに報告することがある（例: 借用を使い
//! 終えた後でも、同じブロック内では生きているとみなす）。領域推論による精密化は今後。
//!
//! 性能: 生存中の借用は **場所（DefId）ごとに索引**して保持する。新しい借用は同じ
//! 場所の借用だけと突き合わせれば済むため、相異なる場所への多数の借用（よくある形）
//! は全体で O(借用数)。スコープ離脱時の破棄も、そのスコープで作った分だけ走査する。

use std::collections::HashMap;

use crate::ast::{Block, Else, Expr, ExprKind, Function, Item, Program, SelfKind, Stmt, UnaryOp};
use crate::sema::resolve::{DefId, DefKind, Resolution};
use crate::sema::ty::Ty;
use crate::sema::typeck::TypeInfo;
use crate::span::Span;

use super::OwnershipError;

pub fn check_borrows(
    program: &Program,
    res: &Resolution,
    type_info: &TypeInfo,
    errors: &mut Vec<OwnershipError>,
) {
    let def_spans: HashMap<Span, DefId> = res
        .defs
        .iter()
        .enumerate()
        .map(|(id, d)| (d.span, id))
        .collect();
    // 固有メソッドの self の受け方（受け手への借用イベントを起こすため）。
    let mut method_self: HashMap<(String, String), SelfKind> = HashMap::new();
    for item in &program.items {
        if let Item::Impl(im) = item {
            for m in &im.methods {
                if let Some(kind) = m.self_kind {
                    method_self.insert((im.type_name.clone(), m.name.clone()), kind);
                }
            }
        }
    }
    let mut bc = Borrows {
        res,
        types: &type_info.expr_types,
        method_self,
        def_spans,
        active: HashMap::new(),
        scopes: Vec::new(),
        binding_root: HashMap::new(),
        errors,
    };
    // 自由関数・固有/トレイトメソッド・トレイト既定実装の本体を検査する。
    let mut bodies: Vec<&Function> = Vec::new();
    for item in &program.items {
        match item {
            Item::Function(f) => bodies.push(f),
            Item::Impl(im) => bodies.extend(im.methods.iter()),
            Item::Trait(tr) => {
                bodies.extend(tr.methods.iter().filter(|m| m.default).map(|m| &m.func));
            }
            Item::TypeDef(_) => {}
        }
    }
    for f in bodies {
        bc.active.clear();
        bc.scopes.clear();
        bc.binding_root.clear();
        bc.visit_block(&f.body);
    }
}

/// 生存中の名前付き借用（宣言ブロックの終わりまで生きる）。`depth` は作られた
/// スコープの深さで、スコープ離脱時の破棄に使う。
struct ActiveBorrow {
    mutable: bool,
    span: Span,
    binding: Option<DefId>,
    depth: usize,
}

/// 1 つの借用イベント（`&place` / `&mut place`）。
struct Event {
    root: DefId,
    mutable: bool,
    span: Span,
}

struct Borrows<'a> {
    res: &'a Resolution,
    /// 式 span → 型（メソッド受け手の型名解決に使う）。
    types: &'a HashMap<Span, Ty>,
    /// 固有メソッドの self の受け方。
    method_self: HashMap<(String, String), SelfKind>,
    def_spans: HashMap<Span, DefId>,
    /// 場所（基底束縛 DefId）→ その場所への生存中の借用。
    active: HashMap<DefId, Vec<ActiveBorrow>>,
    /// スコープごとに、そのスコープで借用を作った場所の一覧（離脱時の破棄用）。
    scopes: Vec<Vec<DefId>>,
    /// 借用束縛 DefId → 現在借用している場所（再代入時の解放に使う）。
    binding_root: HashMap<DefId, DefId>,
    errors: &'a mut Vec<OwnershipError>,
}

impl Borrows<'_> {
    fn visit_block(&mut self, block: &Block) {
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
        // このブロックの深さで作られた借用を破棄する。
        let depth = self.scopes.len();
        let roots = self.scopes.pop().expect("スコープが存在するはず");
        for root in roots {
            if let Some(v) = self.active.get_mut(&root) {
                v.retain(|b| b.depth != depth);
                if v.is_empty() {
                    self.active.remove(&root);
                }
            }
        }
    }

    /// match アームのガード・本体式（式）をスコープ付きで訪問する。
    fn visit_block_arm(&mut self, guard: Option<&Expr>, body: &Expr) {
        self.scopes.push(Vec::new());
        if let Some(g) = guard {
            self.descend_expr(g);
        }
        self.descend_expr(body);
        let depth = self.scopes.len();
        let roots = self.scopes.pop().expect("スコープが存在するはず");
        for root in roots {
            if let Some(v) = self.active.get_mut(&root) {
                v.retain(|b| b.depth != depth);
                if v.is_empty() {
                    self.active.remove(&root);
                }
            }
        }
    }

    /// 名前付き借用を現在のスコープに登録する。
    fn push_borrow(&mut self, root: DefId, mutable: bool, span: Span, binding: Option<DefId>) {
        let depth = self.scopes.len();
        self.active.entry(root).or_default().push(ActiveBorrow {
            mutable,
            span,
            binding,
            depth,
        });
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(root);
        }
        if let Some(b) = binding {
            self.binding_root.insert(b, root);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        // 1. この文に現れる借用イベント（一時借用）を集める。
        let mut temps = Vec::new();
        match stmt {
            Stmt::Let { value, .. } => self.gather(value, &mut temps),
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.gather(v, &mut temps);
                }
            }
            Stmt::Assign { target, value, .. } => {
                self.gather(target, &mut temps);
                self.gather(value, &mut temps);
            }
            // while の条件の一時借用はこの文の間だけ生きる。本体は descend で扱う。
            Stmt::While { cond, .. } => self.gather(cond, &mut temps),
            Stmt::Loop { .. } | Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Expr(e) => self.gather(e, &mut temps),
        }
        // 2. 一時借用同士・生存中の借用との競合を検査する。
        self.check(&temps);

        // 3. `let r = &x` / `r = &mut x` は名前付き借用として生存させる。
        match stmt {
            Stmt::Let { value, span, .. } => {
                if let Some((root, mutable)) = borrow_of(value, self.res) {
                    let binding = self.def_spans.get(span).copied();
                    self.push_borrow(root, mutable, value.span, binding);
                }
            }
            Stmt::Assign { target, value, .. } => {
                // 参照の付け替え（rebind, `r = &mut y`）のときだけ借用を更新する。
                // 参照越し代入（write-through, `r = 5`）は r の借用を生かしたまま
                // 参照先へ書くだけなので、古い借用を解放してはならない（健全性）。
                if let ExprKind::Ident(_) = &target.kind
                    && let Some(&bid) = self.res.uses.get(&target.span)
                    && let Some((root, mutable)) = borrow_of(value, self.res)
                {
                    // 借用変数への再代入は古い借用を解放する。
                    if let Some(old_root) = self.binding_root.remove(&bid)
                        && let Some(v) = self.active.get_mut(&old_root)
                    {
                        v.retain(|b| b.binding != Some(bid));
                        if v.is_empty() {
                            self.active.remove(&old_root);
                        }
                    }
                    self.push_borrow(root, mutable, value.span, Some(bid));
                }
            }
            _ => {}
        }

        // 4. ネストしたブロック（if/else）へスコープを分けて降りる。
        self.descend_stmt(stmt);
    }

    /// 文に含まれる式から借用イベントを集める（if/else ブロックには降りない）。
    fn gather(&self, e: &Expr, out: &mut Vec<Event>) {
        match &e.kind {
            ExprKind::Unary {
                op: op @ (UnaryOp::Ref | UnaryOp::RefMut),
                expr: inner,
            } => {
                if let Some(root) = base_root(inner, self.res) {
                    out.push(Event {
                        root,
                        mutable: matches!(op, UnaryOp::RefMut),
                        span: e.span,
                    });
                }
                self.gather(inner, out);
            }
            ExprKind::Unary { expr, .. } => self.gather(expr, out),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.gather(lhs, out);
                self.gather(rhs, out);
            }
            ExprKind::Call { callee, args } => {
                // メソッド呼び出し `x.m(...)` の受け手は、`&self`/`&mut self` のとき
                // その文の間だけ `x` を借用する（`&mut self` は可変借用）。
                if let ExprKind::Member { object, field, .. } = &callee.kind {
                    match self.receiver_borrow(object, field) {
                        Some(mutable) => {
                            if let Some(root) = base_root(object, self.res) {
                                out.push(Event {
                                    root,
                                    mutable,
                                    span: callee.span,
                                });
                            }
                            self.gather(object, out);
                        }
                        None => self.gather(callee, out),
                    }
                } else {
                    self.gather(callee, out);
                }
                for a in args {
                    self.gather(a, out);
                }
            }
            ExprKind::Member { object, .. } => self.gather(object, out),
            ExprKind::Ternary {
                cond,
                then,
                otherwise,
            } => {
                self.gather(cond, out);
                self.gather(then, out);
                self.gather(otherwise, out);
            }
            ExprKind::Try(inner) => self.gather(inner, out),
            ExprKind::StructLit { fields, .. } => {
                for f in fields {
                    self.gather(&f.value, out);
                }
            }
            // if の条件だけ集め、ブロックは descend で扱う。
            ExprKind::If { cond, .. } => self.gather(cond, out),
            _ => {}
        }
    }

    /// メソッド `object.method` が `&self`/`&mut self` を取るとき、受け手の借用の
    /// 可変性（`&mut self` なら true）を返す。値 self・解決不能は `None`。
    fn receiver_borrow(&self, object: &Expr, method: &str) -> Option<bool> {
        let Ty::Named { name, .. } = self.types.get(&object.span)?.peel_refs() else {
            return None;
        };
        match self.method_self.get(&(name.clone(), method.to_string()))? {
            SelfKind::Ref => Some(false),
            SelfKind::RefMut => Some(true),
            SelfKind::Value => None,
        }
    }

    /// 文に含まれる if/else ブロックへスコープを分けて降りる。
    fn descend_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { value, .. } => self.descend_expr(value),
            Stmt::Return {
                value: Some(v), ..
            } => self.descend_expr(v),
            Stmt::Assign { target, value, .. } => {
                self.descend_expr(target);
                self.descend_expr(value);
            }
            // ループ本体は独立したブロックスコープとして降りる（反復ごとに借用は解放）。
            Stmt::While { cond, body, .. } => {
                self.descend_expr(cond);
                self.visit_block(body);
            }
            Stmt::Loop { body, .. } => self.visit_block(body),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Expr(e) => self.descend_expr(e),
            Stmt::Return { value: None, .. } => {}
        }
    }

    fn descend_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.descend_expr(cond);
                self.visit_block(then);
                if let Some(els) = otherwise {
                    match els.as_ref() {
                        Else::If(e) => self.descend_expr(e),
                        Else::Block(b) => self.visit_block(b),
                    }
                }
            }
            ExprKind::Unary { expr, .. } => self.descend_expr(expr),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.descend_expr(lhs);
                self.descend_expr(rhs);
            }
            ExprKind::Call { callee, args } => {
                self.descend_expr(callee);
                for a in args {
                    self.descend_expr(a);
                }
            }
            ExprKind::Member { object, .. } => self.descend_expr(object),
            ExprKind::Ternary {
                cond,
                then,
                otherwise,
            } => {
                self.descend_expr(cond);
                self.descend_expr(then);
                self.descend_expr(otherwise);
            }
            ExprKind::Try(inner) => self.descend_expr(inner),
            ExprKind::StructLit { fields, .. } => {
                for f in fields {
                    self.descend_expr(&f.value);
                }
            }
            ExprKind::EnumLit { payload, .. } => {
                if let Some(p) = payload {
                    self.descend_expr(p);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.descend_expr(scrutinee);
                for arm in arms {
                    // 各アームは独立したスコープ（反復ごとに借用を解放）。
                    self.visit_block_arm(arm.guard.as_ref(), &arm.body);
                }
            }
            _ => {}
        }
    }

    /// 借用イベント群を、生存中の借用および互いと突き合わせて競合を報告する。
    ///
    /// 生存中の借用は場所ごとに索引してあるので、同じ場所の借用だけを見れば足りる。
    fn check(&mut self, events: &[Event]) {
        for (i, e) in events.iter().enumerate() {
            let mut hits: Vec<(bool, Span)> = Vec::new();
            // 生存中の借用（同じ場所のものだけ）との競合。
            if let Some(v) = self.active.get(&e.root) {
                for b in v {
                    if b.mutable || e.mutable {
                        hits.push((b.mutable, b.span));
                    }
                }
            }
            // 同じ文の先行イベントとの競合（文内のイベントは少数）。
            for prev in &events[..i] {
                if prev.root == e.root && (prev.mutable || e.mutable) {
                    hits.push((prev.mutable, prev.span));
                }
            }
            for (other_mutable, other_span) in hits {
                self.conflict(e, other_mutable, other_span);
            }
        }
    }

    fn conflict(&mut self, e: &Event, other_mutable: bool, other_span: Span) {
        let name = &self.res.defs[e.root].name;
        let message = if e.mutable && other_mutable {
            format!("`{name}` を同時に二重で可変借用しています（`&mut` は一つだけ）")
        } else {
            format!("`{name}` を可変借用と共有借用で同時に借用しています")
        };
        self.errors.push(OwnershipError::with_secondary(
            e.span,
            message,
            other_span,
            "ここで借用しています".to_string(),
        ));
    }
}

/// 式が `&place` / `&mut place` のとき、その場所の基底束縛と可変性を返す。
fn borrow_of(e: &Expr, res: &Resolution) -> Option<(DefId, bool)> {
    if let ExprKind::Unary {
        op: op @ (UnaryOp::Ref | UnaryOp::RefMut),
        expr: inner,
    } = &e.kind
    {
        let root = base_root(inner, res)?;
        return Some((root, matches!(op, UnaryOp::RefMut)));
    }
    None
}

/// 場所式の基底となる束縛（Ident/Member の根）を返す。
fn base_root(place: &Expr, res: &Resolution) -> Option<DefId> {
    match &place.kind {
        ExprKind::Ident(_) => {
            let id = *res.uses.get(&place.span)?;
            matches!(res.defs[id].kind, DefKind::Local | DefKind::Param).then_some(id)
        }
        ExprKind::Member { object, .. } => base_root(object, res),
        _ => None,
    }
}
