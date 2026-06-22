//! 名前解決パス。
//!
//! 各識別子の **使用位置** を、それが指す **定義**（関数・引数・ローカル変数）に
//! 結びつける。スコープを構築しながら走査し、次のエラーを検出する。
//!
//! - 未定義の名前の使用（`未定義の名前 `x``）
//! - トップレベル関数名の重複定義
//! - 同一関数内での引数名の重複
//!
//! ローカル変数の再宣言（シャドーイング）は許可する（`let` の再束縛）。
//! 型名の解決は型定義（struct/enum/type）の実装後に行うため、ここでは扱わない。
//!
//! 検出したエラーは 1 件で止めずすべて収集し、まとめて報告できるようにする。

use std::collections::{HashMap, HashSet};

use crate::ast::{Block, Else, Expr, ExprKind, Function, Item, Pattern, Program, Stmt, TypeDefBody};
use crate::span::Span;

/// 定義の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    /// 言語組み込みの名前（`Ok` などのプレリュード）。
    Builtin,
    Function,
    Param,
    Local,
}

/// プレリュードとして常に利用できる組み込みの値名。
/// Result/Option のコンストラクタ。型定義の実装後に整理する。
const PRELUDE: &[&str] = &["Ok", "Err", "Some", "None"];

/// 名前の定義 1 件。
#[derive(Debug, Clone)]
pub struct Def {
    pub kind: DefKind,
    pub name: String,
    /// 定義（宣言）位置。
    pub span: Span,
    /// 可変かどうか（`let mut` / `&mut` 引数）。将来の可変性検査で使う。
    pub mutable: bool,
}

/// 定義への一意な参照。[`Resolution::defs`] への添字。
pub type DefId = usize;

/// 名前解決の結果。各識別子の使用位置を定義に結びつける。
#[derive(Debug, Default)]
pub struct Resolution {
    /// すべての定義。`DefId` で参照する。
    pub defs: Vec<Def>,
    /// 識別子の使用位置（span）から、それが指す定義への対応。
    pub uses: HashMap<Span, DefId>,
    /// モジュールパス経由の関数呼び出し: callee span → 関数名。
    /// `use std.lib` + `std.lib.fmt(...)` のような呼び出しで使う。
    pub module_fn_calls: HashMap<Span, String>,
}

/// 名前解決エラー。
#[derive(Debug, Clone)]
pub struct ResolveError {
    pub span: Span,
    pub message: String,
}

/// プログラムの名前解決を行う。
///
/// `module_namespaces`: `use a.b`（Plain import）で登録されたパス → pub 関数名集合。
/// 成功すれば [`Resolution`] を、エラーがあればすべての [`ResolveError`] を返す。
pub fn resolve(
    program: &Program,
    module_namespaces: HashMap<Vec<String>, Vec<String>>,
) -> Result<Resolution, Vec<ResolveError>> {
    let mut r = Resolver::default();
    // Vec<String> → HashSet<String> に変換して登録する。
    r.module_namespaces = module_namespaces
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();
    r.run(program);
    if r.errors.is_empty() {
        Ok(Resolution {
            defs: r.defs,
            uses: r.uses,
            module_fn_calls: r.module_fn_calls,
        })
    } else {
        Err(r.errors)
    }
}

/// 1 つのスコープ。名前から定義への対応を持つ。
#[derive(Default)]
struct Scope {
    names: HashMap<String, DefId>,
}

#[derive(Default)]
struct Resolver {
    defs: Vec<Def>,
    uses: HashMap<Span, DefId>,
    /// モジュールパス経由の関数呼び出し: callee span → 関数名。
    module_fn_calls: HashMap<Span, String>,
    scopes: Vec<Scope>,
    errors: Vec<ResolveError>,
    /// `use a.b` で登録されたモジュール名前空間: パス → pub 関数名集合。
    /// `a.b.fmt(...)` 形式の呼び出しを解決するために使う。
    module_namespaces: HashMap<Vec<String>, HashSet<String>>,
}

impl Resolver {
    fn run(&mut self, program: &Program) {
        // グローバルスコープ。組み込みの値名（プレリュード）を先に登録する。
        self.push_scope();
        for name in PRELUDE {
            self.declare(name, DefKind::Builtin, Span::new(0, 0), false, true);
        }
        // 全関数名を登録し、相互再帰・前方参照を許す。
        // 型定義の名前・型名解決は型検査側で扱う（ここでは値の名前のみ）。
        for item in &program.items {
            if let Item::Function(f) = item {
                self.declare(&f.name, DefKind::Function, f.name_span, false, false);
            }
        }
        // enum のバリアント名をグローバルスコープに登録する（ジェネリック含む）。
        // バリアントはコンストラクタ（値）として使えるため値の名前空間に入れる。
        for item in &program.items {
            if let Item::TypeDef(t) = item
                && let TypeDefBody::Enum(variants) = &t.body
            {
                for v in variants {
                    // シャドーイングを許す（prelude の Ok/Err/Some/None は上書き可）。
                    self.declare(&v.name, DefKind::Builtin, v.span, false, true);
                }
            }
        }
        for item in &program.items {
            match item {
                Item::Function(f) => self.resolve_function(f),
                // impl のメソッドは値の名前空間には登録しない（`x.m()` で型から解決）。
                // 本体だけ解決する。`self` は合成された引数として登録される。
                Item::Impl(im) => {
                    for m in &im.methods {
                        self.resolve_function(m);
                    }
                }
                // トレイトの既定実装（本体付きメソッド）だけ解決する。
                // メソッド名は値の名前空間に登録しない（`x.m()` で型から解決）。
                Item::Trait(tr) => {
                    for m in &tr.methods {
                        if m.default {
                            self.resolve_function(&m.func);
                        }
                    }
                }
                Item::TypeDef(_) => {}
                // use 宣言は lib.rs のモジュールローダーが処理済み。ここでは無視する。
                Item::Use(_) => {}
            }
        }
        self.pop_scope();
    }

    fn resolve_function(&mut self, f: &Function) {
        // 関数本体のスコープに引数を登録する。引数名の重複はエラー。
        self.push_scope();
        for p in &f.params {
            let mutable = matches!(&p.ty, crate::ast::Type::Ref { mutable: true, .. });
            self.declare(&p.name, DefKind::Param, p.span, mutable, false);
        }
        // 引数の型・戻り値型の名前解決は型定義の実装後に行う。
        self.resolve_block(&f.body);
        self.pop_scope();
    }

    fn resolve_block(&mut self, block: &Block) {
        self.push_scope();
        for stmt in &block.stmts {
            self.resolve_stmt(stmt);
        }
        self.pop_scope();
    }

    fn resolve_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                mutable,
                name,
                value,
                span,
                ..
            } => {
                // RHS を先に解決してから束縛する（自己参照を防ぐ）。
                self.resolve_expr(value);
                self.declare(name, DefKind::Local, *span, *mutable, true);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.resolve_expr(v);
                }
            }
            Stmt::Assign { target, value, .. } => {
                self.resolve_expr(target);
                self.resolve_expr(value);
            }
            Stmt::While { cond, body, .. } => {
                self.resolve_expr(cond);
                self.resolve_block(body);
            }
            Stmt::Loop { body, .. } => self.resolve_block(body),
            Stmt::For {
                var,
                var_span,
                start,
                end,
                body,
                ..
            } => {
                // 範囲の境界は本体スコープの外で解決する。
                self.resolve_expr(start);
                self.resolve_expr(end);
                // ループ変数は本体を囲む独立スコープへ束縛する（不変・反復ごとに再束縛）。
                self.push_scope();
                self.declare(var, DefKind::Local, *var_span, false, true);
                self.resolve_block(body);
                self.pop_scope();
            }
            Stmt::ForIn {
                var,
                var_span,
                iter,
                body,
                ..
            } => {
                // イテレータ式は本体スコープの外で解決する。
                self.resolve_expr(iter);
                // ループ変数は本体を囲む独立スコープへ束縛する（反復ごとに再束縛）。
                self.push_scope();
                self.declare(var, DefKind::Local, *var_span, false, true);
                self.resolve_block(body);
                self.pop_scope();
            }
            // break / continue は名前を持たない（ループ外使用の検査は型検査で行う）。
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Expr(e) => self.resolve_expr(e),
        }
    }

    fn resolve_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_) => {}
            ExprKind::Ident(name) => self.resolve_name(name, expr.span),
            ExprKind::Unary { expr: inner, .. } => self.resolve_expr(inner),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.resolve_expr(lhs);
                self.resolve_expr(rhs);
            }
            ExprKind::Call { callee, args } => {
                self.resolve_expr(callee);
                for a in args {
                    self.resolve_expr(a);
                }
            }
            // メンバアクセス `a.b` または モジュールパス `std.lib.fmt`。
            // モジュールパス呼び出しを先に試み、失敗なら通常のフィールドアクセスとして扱う。
            ExprKind::Member { object, field, .. } => {
                // `object.field` の object 以下をドット連鎖として取り出す。
                if let Some(mut path) = Self::extract_ident_chain(object) {
                    path.push(field.clone());
                    // path[0..n-1] がモジュールパスで、path[n-1] がそのエクスポートか確認する。
                    if path.len() >= 2 {
                        let module_path = &path[..path.len() - 1];
                        let fn_name = path.last().unwrap();
                        if let Some(ns) = self.module_namespaces.get(module_path) {
                            if ns.contains(fn_name.as_str()) {
                                // モジュールパス経由の呼び出しとして記録する。
                                self.module_fn_calls.insert(expr.span, fn_name.clone());
                                return;
                            }
                        }
                    }
                }
                // 通常のメンバアクセス: object だけ解決する（field は型情報が必要）。
                self.resolve_expr(object);
            }
            ExprKind::Ternary {
                cond,
                then,
                otherwise,
            } => {
                self.resolve_expr(cond);
                self.resolve_expr(then);
                self.resolve_expr(otherwise);
            }
            ExprKind::Try(inner) => self.resolve_expr(inner),
            // 構造体リテラル。型名は型検査で検証し、ここでは初期化式のみ解決する。
            ExprKind::StructLit { fields, .. } => {
                for f in fields {
                    self.resolve_expr(&f.value);
                }
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.resolve_expr(cond);
                self.resolve_block(then);
                if let Some(els) = otherwise {
                    match els.as_ref() {
                        Else::If(e) => self.resolve_expr(e),
                        Else::Block(b) => self.resolve_block(b),
                    }
                }
            }
            ExprKind::EnumLit { payload, .. } => {
                if let Some(p) = payload {
                    self.resolve_expr(p);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.resolve_expr(scrutinee);
                for arm in arms {
                    // アームの本体は独立したスコープ。パターンの束縛変数を登録する。
                    self.push_scope();
                    if let Pattern::Variant {
                        binding: Some((bname, bspan)),
                        ..
                    } = &arm.pattern
                    {
                        self.declare(bname, DefKind::Local, *bspan, false, true);
                    }
                    // ガードはアームスコープ内で解決する（束縛変数を参照できる）。
                    if let Some(g) = &arm.guard {
                        self.resolve_expr(g);
                    }
                    self.resolve_expr(&arm.body);
                    self.pop_scope();
                }
            }
            ExprKind::ArrayLit { elems } => {
                for e in elems {
                    self.resolve_expr(e);
                }
            }
            ExprKind::Index { base, index } => {
                self.resolve_expr(base);
                self.resolve_expr(index);
            }
            // `expr as Type`。型名は型検査で検証し、ここでは被変換式のみ解決する。
            ExprKind::Cast { expr: inner, .. } => self.resolve_expr(inner),
        }
    }

    /// `Member(Member(...Ident("a"), "b"), "c")` のような純粋なドット連鎖から
    /// セグメント列 `["a", "b", "c"]` を取り出す。連鎖でない式は `None` を返す。
    fn extract_ident_chain(expr: &Expr) -> Option<Vec<String>> {
        match &expr.kind {
            ExprKind::Ident(name) => Some(vec![name.clone()]),
            ExprKind::Member {
                object,
                field,
                qualifier: None,
            } => {
                let mut segs = Self::extract_ident_chain(object)?;
                segs.push(field.clone());
                Some(segs)
            }
            _ => None,
        }
    }

    /// 名前を現在のスコープ列から探し、使用位置を定義に結びつける。
    /// 見つからなければエラーを記録する。
    fn resolve_name(&mut self, name: &str, span: Span) {
        for scope in self.scopes.iter().rev() {
            if let Some(&id) = scope.names.get(name) {
                self.uses.insert(span, id);
                return;
            }
        }
        self.errors.push(ResolveError {
            span,
            message: format!("未定義の名前 `{name}`"),
        });
    }

    /// 名前を現在のスコープに登録する。
    ///
    /// `allow_shadow` が偽で同一スコープに同名がある場合は重複定義エラーにする
    /// （関数名・引数名）。真の場合は再束縛を許す（ローカル変数）。
    fn declare(
        &mut self,
        name: &str,
        kind: DefKind,
        span: Span,
        mutable: bool,
        allow_shadow: bool,
    ) -> DefId {
        let id = self.defs.len();
        self.defs.push(Def {
            kind,
            name: name.to_string(),
            span,
            mutable,
        });

        let scope = self.scopes.last_mut().expect("スコープが存在するはず");
        if !allow_shadow && scope.names.contains_key(name) {
            let what = match kind {
                DefKind::Builtin => "組み込み名",
                DefKind::Function => "関数",
                DefKind::Param => "引数",
                DefKind::Local => "変数",
            };
            self.errors.push(ResolveError {
                span,
                message: format!("{what} `{name}` が二重に定義されています"),
            });
        }
        scope.names.insert(name.to_string(), id);
        id
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}
