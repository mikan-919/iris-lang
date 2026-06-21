//! 型検査パス。
//!
//! 名前解決 ([`crate::sema::resolve`]) の結果を前提に、式・文・関数へ型を付け、
//! 整合性を検査する。検出するもの:
//!
//! - 代入・引数・戻り値・`let` 注釈の型不一致
//! - 数値演算・比較・論理演算の被演算子型
//! - 関数呼び出しの引数個数・型
//! - `!`（エラー伝播）を Result/Option 以外へ適用、または Result/Option を
//!   返さない関数内での使用
//! - 不変な束縛（`let`（mut なし）/ `const`）への再代入
//! - 未定義の型名、構造体リテラル/メンバアクセスのフィールド整合性
//!
//! 整数・浮動小数リテラルは未確定型として扱い、文脈の具体型へ適合させる。
//! 注釈の無い `let` ではリテラルを既定型（`i32` / `f64`）へ確定する。
//!
//! `type` 別名は名前的型付け（別の型）として扱うが、リテラル代入の可否は
//! 別名の元の型で判定する（`type Meters = f64` に `5.0` は代入可、`f64` 値は不可）。
//! ジェネリックな型定義の本体・本体内のメンバ型は単一化を実装していないため
//! 寛容に（`Infer`）扱う。

use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinaryOp, Block, Else, Expr, ExprKind, FieldInit, Function, Item, Program, Stmt, Type, TypeDef,
    TypeDefBody, UnaryOp,
};
use crate::sema::resolve::{DefId, DefKind, Resolution};
use crate::sema::ty::{FLOAT_TYPES, INT_TYPES, Ty};
use crate::span::Span;

/// プリミティブ以外の組み込み型名（ジェネリックなものを含む）。
const BUILTIN_TYPES: &[&str] = &[
    "bool", "string", "char", "void", "Vec", "Result", "Option", "Box", "Map", "Set",
];

/// 型検査の結果。各式の型を保持する（今後のコード生成で利用する）。
#[derive(Debug, Default)]
pub struct TypeInfo {
    pub expr_types: HashMap<Span, Ty>,
}

/// 型エラー。
#[derive(Debug, Clone)]
pub struct TypeError {
    pub span: Span,
    pub message: String,
}

/// 関数のシグネチャ。
struct FuncSig {
    params: Vec<Ty>,
    ret: Ty,
}

/// 型定義の情報。
enum TyDef {
    /// `type X = T`（名前的型付け）。元の型を持つ。
    Alias(Ty),
    /// `type X = struct { ... }`。型パラメータ数とフィールド（名前→型）。
    Struct {
        generics: usize,
        fields: Vec<(String, Ty)>,
    },
    /// `type X = enum { ... }`。バリアントの構築/分解構文は未対応。
    Enum,
}

/// プログラムの型検査を行う。
pub fn check(program: &Program, res: &Resolution) -> Result<TypeInfo, Vec<TypeError>> {
    let mut checker = Checker::new(program, res);
    for item in &program.items {
        if let Item::Function(f) = item {
            checker.check_function(f);
        }
    }
    if checker.errors.is_empty() {
        Ok(TypeInfo {
            expr_types: checker.expr_types,
        })
    } else {
        Err(checker.errors)
    }
}

struct Checker<'a> {
    res: &'a Resolution,
    funcs: HashMap<String, FuncSig>,
    /// 型定義（別名・struct・enum）。
    types: HashMap<String, TyDef>,
    /// 既知の型名（プリミティブ＋組み込み＋定義済み）。型名検証に使う。
    known_types: HashSet<String>,
    /// DefId ごとの型。引数・ローカルの型を記録する。
    def_types: Vec<Ty>,
    /// 定義（宣言）位置の span から DefId を引くための表。
    def_spans: HashMap<Span, DefId>,
    expr_types: HashMap<Span, Ty>,
    errors: Vec<TypeError>,
    /// 検査中の関数の戻り値型。
    current_ret: Ty,
    /// 現在ネストしているループの深さ（`break`/`continue` のループ外使用の検出に使う）。
    loop_depth: usize,
}

impl<'a> Checker<'a> {
    fn new(program: &Program, res: &'a Resolution) -> Self {
        // 既知の型名の集合。
        let mut known_types: HashSet<String> = INT_TYPES
            .iter()
            .chain(FLOAT_TYPES)
            .chain(BUILTIN_TYPES)
            .map(|s| s.to_string())
            .collect();
        for item in &program.items {
            if let Item::TypeDef(t) = item {
                known_types.insert(t.name.clone());
            }
        }

        // 型定義テーブル。
        let mut types = HashMap::new();
        for item in &program.items {
            if let Item::TypeDef(t) = item {
                types.insert(t.name.clone(), lower_type_def(t));
            }
        }

        // 関数シグネチャ。
        let mut funcs = HashMap::new();
        for item in &program.items {
            if let Item::Function(f) = item {
                let params = f.params.iter().map(|p| Ty::from_ast(&p.ty)).collect();
                let ret = f.ret.as_ref().map_or_else(Ty::unit, Ty::from_ast);
                funcs.insert(f.name.clone(), FuncSig { params, ret });
            }
        }

        // 定義位置 span → DefId。ビルトインの span (0,0) は引かないため衝突しても無害。
        let def_spans = res
            .defs
            .iter()
            .enumerate()
            .map(|(id, def)| (def.span, id))
            .collect();

        Checker {
            res,
            funcs,
            types,
            known_types,
            def_types: vec![Ty::Infer; res.defs.len()],
            def_spans,
            expr_types: HashMap::new(),
            errors: Vec::new(),
            current_ret: Ty::unit(),
            loop_depth: 0,
        }
    }

    fn error(&mut self, span: Span, message: String) {
        self.errors.push(TypeError { span, message });
    }

    // ---- 型名の検証 -----------------------------------------------------

    /// ユーザーが書いた型注釈の名前が既知か検証する（未定義の型名を報告）。
    fn validate_type(&mut self, t: &Type) {
        match t {
            Type::Named { name, args, span } => {
                if !self.known_types.contains(name) {
                    self.error(*span, format!("未定義の型 `{name}`"));
                }
                for a in args {
                    self.validate_type(a);
                }
            }
            Type::Ref { inner, .. } => self.validate_type(inner),
            Type::Array { inner, .. } => self.validate_type(inner),
            Type::Tuple { elems, .. } => {
                for e in elems {
                    self.validate_type(e);
                }
            }
        }
    }

    fn check_function(&mut self, f: &Function) {
        // 引数・戻り値の型名を検証し、引数型を DefId に登録する。
        for p in &f.params {
            self.validate_type(&p.ty);
            if let Some(&id) = self.def_spans.get(&p.span) {
                self.def_types[id] = Ty::from_ast(&p.ty);
            }
        }
        if let Some(ret) = &f.ret {
            self.validate_type(ret);
        }
        self.current_ret = self
            .funcs
            .get(&f.name)
            .map_or_else(Ty::unit, |s| s.ret.clone());
        self.check_block(&f.body);
    }

    fn check_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.check_stmt(stmt);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                ty, value, span, ..
            } => {
                let value_ty = self.check_expr(value);
                let bind_ty = match ty {
                    Some(annot) => {
                        self.validate_type(annot);
                        let expected = Ty::from_ast(annot);
                        if !self.assignable(&expected, &value_ty) {
                            self.error(
                                value.span,
                                format!(
                                    "型が一致しません: `{}` を期待しましたが `{}` でした",
                                    expected.describe(),
                                    value_ty.describe()
                                ),
                            );
                        }
                        expected
                    }
                    // 注釈なしはリテラルを既定型へ確定する。
                    None => value_ty.defaulted(),
                };
                if let Some(&id) = self.def_spans.get(span) {
                    self.def_types[id] = bind_ty;
                }
            }
            Stmt::Return { value, span } => {
                let ret = self.current_ret.clone();
                match value {
                    Some(v) => {
                        let vty = self.check_expr(v);
                        if !self.assignable(&ret, &vty) {
                            self.error(
                                v.span,
                                format!(
                                    "戻り値の型が一致しません: `{}` を期待しましたが `{}` でした",
                                    ret.describe(),
                                    vty.describe()
                                ),
                            );
                        }
                    }
                    None => {
                        if !self.assignable(&ret, &Ty::unit()) {
                            self.error(
                                *span,
                                format!("戻り値 `{}` が必要ですが、値がありません", ret.describe()),
                            );
                        }
                    }
                }
            }
            Stmt::Assign { target, value, .. } => {
                let target_ty = self.check_expr(target);
                let value_ty = self.check_expr(value);
                // 参照越し代入（write-through）: 代入先が参照型 `&mut T` で、右辺が
                // 値型 T のとき、参照先へ書き込む。右辺が参照型なら束縛の付け替え
                // （rebind）として target_ty 全体で判定する（RHS の型で振り分ける）。
                if let Ty::Ref { mutable, inner } = &target_ty
                    && !matches!(value_ty, Ty::Ref { .. })
                {
                    // 可変性は参照（`&mut`）に依存する。束縛 r 自体の mut は不要。
                    if !*mutable {
                        self.error(
                            target.span,
                            "不変参照 `&T` の参照先には書き込めません（`&mut` が必要）"
                                .to_string(),
                        );
                    } else if !self.assignable(inner, &value_ty) {
                        self.error(
                            value.span,
                            format!(
                                "代入の型が一致しません: `{}` に `{}` は代入できません",
                                inner.describe(),
                                value_ty.describe()
                            ),
                        );
                    }
                } else {
                    self.check_assignable_target(target);
                    if !self.assignable(&target_ty, &value_ty) {
                        self.error(
                            value.span,
                            format!(
                                "代入の型が一致しません: `{}` に `{}` は代入できません",
                                target_ty.describe(),
                                value_ty.describe()
                            ),
                        );
                    }
                }
            }
            Stmt::While { cond, body, .. } => {
                let c = self.check_expr(cond);
                self.expect_bool(&c, cond.span, "while の条件");
                self.loop_depth += 1;
                self.check_block(body);
                self.loop_depth -= 1;
            }
            Stmt::Loop { body, .. } => {
                self.loop_depth += 1;
                self.check_block(body);
                self.loop_depth -= 1;
            }
            Stmt::Break { span } => {
                if self.loop_depth == 0 {
                    self.error(*span, "`break` はループの中でのみ使えます".to_string());
                }
            }
            Stmt::Continue { span } => {
                if self.loop_depth == 0 {
                    self.error(*span, "`continue` はループの中でのみ使えます".to_string());
                }
            }
            Stmt::Expr(e) => {
                self.check_expr(e);
            }
        }
    }

    /// 代入先が可変な束縛か検査する。
    fn check_assignable_target(&mut self, target: &Expr) {
        if let ExprKind::Ident(name) = &target.kind
            && let Some(&id) = self.res.uses.get(&target.span)
        {
            let def = &self.res.defs[id];
            if def.kind == DefKind::Local && !def.mutable {
                self.error(
                    target.span,
                    format!("不変な変数 `{name}` には再代入できません（`let mut` が必要）"),
                );
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> Ty {
        let ty = self.infer_expr(expr);
        self.expr_types.insert(expr.span, ty.clone());
        ty
    }

    fn infer_expr(&mut self, expr: &Expr) -> Ty {
        match &expr.kind {
            ExprKind::Int(_) => Ty::IntLit,
            ExprKind::Float(_) => Ty::FloatLit,
            ExprKind::Str(_) => Ty::named("string"),
            ExprKind::Bool(_) => Ty::named("bool"),
            ExprKind::Ident(_) => self
                .res
                .uses
                .get(&expr.span)
                .map_or(Ty::Infer, |&id| self.def_types[id].clone()),
            ExprKind::Unary { op, expr: inner } => self.infer_unary(*op, inner),
            ExprKind::Binary { op, lhs, rhs } => self.infer_binary(*op, lhs, rhs),
            ExprKind::Call { callee, args } => self.infer_call(callee, args),
            ExprKind::Member { object, field } => self.infer_member(object, field, expr.span),
            ExprKind::Ternary {
                cond,
                then,
                otherwise,
            } => self.infer_ternary(cond, then, otherwise),
            ExprKind::Try(inner) => self.infer_try(inner),
            ExprKind::StructLit {
                name,
                name_span,
                fields,
            } => self.infer_struct_lit(name, *name_span, fields, expr.span),
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                let c = self.check_expr(cond);
                self.expect_bool(&c, cond.span, "if の条件");
                self.check_block(then);
                if let Some(els) = otherwise {
                    match els.as_ref() {
                        Else::If(e) => {
                            self.check_expr(e);
                        }
                        Else::Block(b) => self.check_block(b),
                    }
                }
                // ブロックは値を持たないため if 式は void。
                Ty::unit()
            }
        }
    }

    fn infer_unary(&mut self, op: UnaryOp, inner: &Expr) -> Ty {
        let t = self.check_expr(inner);
        match op {
            UnaryOp::Neg => {
                if !t.is_numeric() && t != Ty::Error && t != Ty::Infer {
                    self.error(
                        inner.span,
                        format!("単項 `-` は数値型に使えますが `{}` でした", t.describe()),
                    );
                    return Ty::Error;
                }
                t
            }
            UnaryOp::Ref => Ty::Ref {
                mutable: false,
                inner: Box::new(t),
            },
            UnaryOp::RefMut => Ty::Ref {
                mutable: true,
                inner: Box::new(t),
            },
        }
    }

    fn infer_binary(&mut self, op: BinaryOp, lhs: &Expr, rhs: &Expr) -> Ty {
        let l = self.check_expr(lhs);
        let r = self.check_expr(rhs);
        let span = lhs.span.merge(rhs.span);
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                match self.join_numeric(&l, &r) {
                    Some(t) => t,
                    None => {
                        self.error(
                            span,
                            format!(
                                "算術演算の型が不正です: `{}` と `{}`",
                                l.describe(),
                                r.describe()
                            ),
                        );
                        Ty::Error
                    }
                }
            }
            BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                if self.join_numeric(&l, &r).is_none() {
                    self.error(
                        span,
                        format!("比較の型が不正です: `{}` と `{}`", l.describe(), r.describe()),
                    );
                }
                Ty::named("bool")
            }
            BinaryOp::Eq | BinaryOp::NotEq => {
                if !self.assignable(&l, &r) && !self.assignable(&r, &l) {
                    self.error(
                        span,
                        format!(
                            "等価比較の型が一致しません: `{}` と `{}`",
                            l.describe(),
                            r.describe()
                        ),
                    );
                }
                Ty::named("bool")
            }
            BinaryOp::And | BinaryOp::Or => {
                self.expect_bool(&l, lhs.span, "論理演算");
                self.expect_bool(&r, rhs.span, "論理演算");
                Ty::named("bool")
            }
        }
    }

    fn infer_call(&mut self, callee: &Expr, args: &[Expr]) -> Ty {
        let arg_tys: Vec<Ty> = args.iter().map(|a| self.check_expr(a)).collect();

        if let ExprKind::Ident(name) = &callee.kind
            && let Some(&id) = self.res.uses.get(&callee.span)
        {
            match self.res.defs[id].kind {
                DefKind::Function => return self.check_func_call(name, callee.span, args, &arg_tys),
                DefKind::Builtin => return builtin_ctor(name, &arg_tys),
                _ => {}
            }
        }
        // 関数値・メンバ呼び出しなどは型情報が無いため不明とする。
        self.check_expr(callee);
        Ty::Infer
    }

    fn check_func_call(&mut self, name: &str, span: Span, args: &[Expr], arg_tys: &[Ty]) -> Ty {
        let Some(sig) = self.funcs.get(name) else {
            return Ty::Infer;
        };
        let ret = sig.ret.clone();
        let params = sig.params.clone();
        if params.len() != arg_tys.len() {
            self.error(
                span,
                format!(
                    "`{name}` の引数は {} 個ですが {} 個渡されました",
                    params.len(),
                    arg_tys.len()
                ),
            );
            return ret;
        }
        for ((expected, actual), arg) in params.iter().zip(arg_tys).zip(args) {
            if !self.assignable(expected, actual) {
                self.error(
                    arg.span,
                    format!(
                        "引数の型が一致しません: `{}` を期待しましたが `{}` でした",
                        expected.describe(),
                        actual.describe()
                    ),
                );
            }
        }
        ret
    }

    /// メンバアクセス `object.field` の型を求める。
    fn infer_member(&mut self, object: &Expr, field: &str, span: Span) -> Ty {
        let mut obj = self.check_expr(object);
        // 参照は自動でたどる。
        while let Ty::Ref { inner, .. } = obj {
            obj = *inner;
        }
        if let Ty::Named { name, .. } = &obj
            && let Some((generics, fields)) = self.struct_fields(name)
        {
            return match fields.iter().find(|(n, _)| n == field) {
                // ジェネリック struct はフィールド型を単一化できないため Infer。
                Some(_) if generics > 0 => Ty::Infer,
                Some((_, fty)) => fty.clone(),
                None => {
                    self.error(
                        span,
                        format!("型 `{}` にフィールド `{field}` はありません", obj.describe()),
                    );
                    Ty::Error
                }
            };
        }
        // struct 以外（組み込み型・タプル等）のメンバは追跡できないため寛容に扱う。
        Ty::Infer
    }

    /// 構造体リテラル `Name { ... }` の型を求める。
    fn infer_struct_lit(
        &mut self,
        name: &str,
        name_span: Span,
        fields: &[FieldInit],
        span: Span,
    ) -> Ty {
        if !self.known_types.contains(name) {
            self.error(name_span, format!("未定義の型 `{name}`"));
        }

        let Some((generics, def_fields)) = self.struct_fields(name) else {
            // struct でない／未知の型。初期化式だけ検査する。
            for fi in fields {
                self.check_expr(&fi.value);
            }
            if self.known_types.contains(name) {
                self.error(name_span, format!("`{name}` は構造体ではありません"));
            }
            return Ty::Error;
        };

        let mut seen: HashSet<&str> = HashSet::new();
        for fi in fields {
            let vty = self.check_expr(&fi.value);
            if !seen.insert(&fi.name) {
                self.error(fi.name_span, format!("フィールド `{}` が重複しています", fi.name));
                continue;
            }
            match def_fields.iter().find(|(n, _)| n == &fi.name) {
                Some((_, fty)) => {
                    // ジェネリック struct はフィールド型検査を省略する。
                    if generics == 0 && !self.assignable(fty, &vty) {
                        self.error(
                            fi.value.span,
                            format!(
                                "フィールド `{}` の型が一致しません: `{}` を期待しましたが `{}` でした",
                                fi.name,
                                fty.describe(),
                                vty.describe()
                            ),
                        );
                    }
                }
                None => self.error(
                    fi.name_span,
                    format!("型 `{name}` にフィールド `{}` はありません", fi.name),
                ),
            }
        }
        // 未初期化フィールドの検出。
        for (fname, _) in &def_fields {
            if !fields.iter().any(|fi| &fi.name == fname) {
                self.error(span, format!("フィールド `{fname}` が初期化されていません"));
            }
        }

        Ty::Named {
            name: name.to_string(),
            args: vec![Ty::Infer; generics],
        }
    }

    fn infer_ternary(&mut self, cond: &Expr, then: &Expr, otherwise: &Expr) -> Ty {
        let c = self.check_expr(cond);
        self.expect_bool(&c, cond.span, "三項演算子の条件");
        let t = self.check_expr(then);
        let e = self.check_expr(otherwise);
        match self.join(&t, &e) {
            Some(ty) => ty,
            None => {
                self.error(
                    then.span.merge(otherwise.span),
                    format!(
                        "三項演算子の分岐の型が一致しません: `{}` と `{}`",
                        t.describe(),
                        e.describe()
                    ),
                );
                Ty::Error
            }
        }
    }

    fn infer_try(&mut self, inner: &Expr) -> Ty {
        let t = self.check_expr(inner);
        // 現在の関数が Result/Option を返さなければ `!` は使えない。
        let ret_ok =
            matches!(&self.current_ret, Ty::Infer | Ty::Error) || is_result_or_option(&self.current_ret);
        if !ret_ok {
            self.error(
                inner.span,
                "`!` は Result または Option を返す関数の中でのみ使えます".to_string(),
            );
        }
        match &t {
            Ty::Infer | Ty::Error => Ty::Infer,
            Ty::Named { name, args } if (name == "Result" || name == "Option") => {
                args.first().cloned().unwrap_or(Ty::Infer)
            }
            other => {
                self.error(
                    inner.span,
                    format!(
                        "`!` は Result または Option にのみ使えますが `{}` でした",
                        other.describe()
                    ),
                );
                Ty::Error
            }
        }
    }

    fn expect_bool(&mut self, t: &Ty, span: Span, ctx: &str) {
        // 参照は暗黙にデリファレンスされる（`&bool` は条件として使える）。
        let t = t.peel_refs();
        if !t.is_bool() && *t != Ty::Error && *t != Ty::Infer {
            self.error(
                span,
                format!("{ctx} は bool が必要ですが `{}` でした", t.describe()),
            );
        }
    }

    // ---- 型定義の参照ヘルパ ---------------------------------------------

    /// 名前を別名チェーンでたどり、struct なら（型パラメータ数, フィールド）を返す。
    fn struct_fields(&self, name: &str) -> Option<(usize, Vec<(String, Ty)>)> {
        let mut cur = name.to_string();
        for _ in 0..32 {
            match self.types.get(&cur)? {
                TyDef::Struct { generics, fields } => return Some((*generics, fields.clone())),
                TyDef::Alias(Ty::Named { name, .. }) => cur = name.clone(),
                _ => return None,
            }
        }
        None
    }

    /// 別名の元の型を返す。
    fn alias_target(&self, name: &str) -> Option<&Ty> {
        match self.types.get(name)? {
            TyDef::Alias(t) => Some(t),
            _ => None,
        }
    }

    /// 整数族か（別名は元をたどる）。リテラル適合の判定に使う。
    fn is_integer_like(&self, ty: &Ty) -> bool {
        match ty {
            Ty::IntLit => true,
            Ty::Named { name, args } if args.is_empty() => {
                INT_TYPES.contains(&name.as_str())
                    || self.alias_target(name).is_some_and(|t| self.is_integer_like(t))
            }
            _ => false,
        }
    }

    /// 浮動小数族か（別名は元をたどる）。
    fn is_float_like(&self, ty: &Ty) -> bool {
        match ty {
            Ty::FloatLit => true,
            Ty::Named { name, args } if args.is_empty() => {
                FLOAT_TYPES.contains(&name.as_str())
                    || self.alias_target(name).is_some_and(|t| self.is_float_like(t))
            }
            _ => false,
        }
    }

    // ---- 適合判定 -------------------------------------------------------

    /// `actual` 型の値を `expected` 型の場所で使えるか。
    /// `Infer`/`Error` はどちらに現れても適合し、リテラルは対応する具体型（別名含む）へ適合する。
    /// 構造（Named の名前など）は名前的に比較するため、別名は元の型と区別される。
    fn assignable(&self, expected: &Ty, actual: &Ty) -> bool {
        use Ty::*;
        if matches!(expected, Infer | Error) || matches!(actual, Infer | Error) {
            return true;
        }
        if *actual == IntLit && self.is_integer_like(expected) {
            return true;
        }
        if *expected == IntLit && self.is_integer_like(actual) {
            return true;
        }
        if *actual == FloatLit && self.is_float_like(expected) {
            return true;
        }
        if *expected == FloatLit && self.is_float_like(actual) {
            return true;
        }
        // 暗黙デリファレンス: `&T` の値は `T` が期待される場所で自動的に
        // 参照外しされる（`&mut T` → `&T` の参照同士の適合は下の match で扱う）。
        if let Ty::Ref { inner, .. } = actual
            && !matches!(expected, Ty::Ref { .. })
        {
            return self.assignable(expected, inner);
        }
        match (expected, actual) {
            (Named { name: n1, args: a1 }, Named { name: n2, args: a2 }) => {
                n1 == n2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2).all(|(x, y)| self.assignable(x, y))
            }
            // &mut T は &T として使えるが、逆は不可。
            (Ref { mutable: m1, inner: i1 }, Ref { mutable: m2, inner: i2 }) => {
                (!*m1 || *m2) && self.assignable(i1, i2)
            }
            (Array(i1), Array(i2)) => self.assignable(i1, i2),
            (Tuple(e1), Tuple(e2)) => {
                e1.len() == e2.len() && e1.iter().zip(e2).all(|(x, y)| self.assignable(x, y))
            }
            _ => false,
        }
    }

    /// 両方向に適合する 2 型から共通の型を求める（三項演算子の分岐など）。
    fn join(&self, a: &Ty, b: &Ty) -> Option<Ty> {
        if self.assignable(a, b) {
            Some(pick_concrete(a, b))
        } else if self.assignable(b, a) {
            Some(pick_concrete(b, a))
        } else {
            None
        }
    }

    /// 数値 2 項演算の結果型。両者が同じ数値族のときのみ成立する。
    /// 参照（`&T`）の被演算子は暗黙にデリファレンスして判定する。
    fn join_numeric(&self, a: &Ty, b: &Ty) -> Option<Ty> {
        let a = a.peel_refs();
        let b = b.peel_refs();
        if matches!(a, Ty::Error | Ty::Infer) {
            return Some(b.clone());
        }
        if matches!(b, Ty::Error | Ty::Infer) {
            return Some(a.clone());
        }
        if self.is_integer_like(a) && self.is_integer_like(b) {
            return Some(pick_concrete(a, b));
        }
        if self.is_float_like(a) && self.is_float_like(b) {
            return Some(pick_concrete(a, b));
        }
        None
    }
}

/// AST の型定義を内部表現へ変換する。
fn lower_type_def(t: &TypeDef) -> TyDef {
    match &t.body {
        TypeDefBody::Alias(ty) => TyDef::Alias(Ty::from_ast(ty)),
        TypeDefBody::Struct(fields) => TyDef::Struct {
            generics: t.generics.len(),
            fields: fields
                .iter()
                .map(|f| (f.name.clone(), Ty::from_ast(&f.ty)))
                .collect(),
        },
        TypeDefBody::Enum(_) => TyDef::Enum,
    }
}

/// Result/Option コンストラクタ（プレリュード）の型を組み立てる。
fn builtin_ctor(name: &str, arg_tys: &[Ty]) -> Ty {
    let first = arg_tys.first().cloned().unwrap_or(Ty::Infer);
    match name {
        "Ok" => Ty::Named {
            name: "Result".into(),
            args: vec![first, Ty::Infer],
        },
        "Err" => Ty::Named {
            name: "Result".into(),
            args: vec![Ty::Infer, first],
        },
        "Some" => Ty::Named {
            name: "Option".into(),
            args: vec![first],
        },
        "None" => Ty::Named {
            name: "Option".into(),
            args: vec![Ty::Infer],
        },
        _ => Ty::Infer,
    }
}

/// 適合する 2 型のうち、より具体的な方を選ぶ。
fn pick_concrete(a: &Ty, b: &Ty) -> Ty {
    let vague = |t: &Ty| matches!(t, Ty::Infer | Ty::IntLit | Ty::FloatLit | Ty::Error);
    if vague(a) && !vague(b) { b.clone() } else { a.clone() }
}

fn is_result_or_option(t: &Ty) -> bool {
    matches!(t, Ty::Named { name, .. } if name == "Result" || name == "Option")
}
