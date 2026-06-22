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
//! ジェネリック struct/enum は構築箇所のフィールド/ペイロード値から型パラメータを
//! `unify` で推論し、メンバ/バリアントの型を型引数で単相化する。

use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinaryOp, Block, Else, Expr, ExprKind, FieldInit, Function, Item, LitPat, MatchArm, Pattern,
    Program, SelfKind, Stmt, TraitRef, Type, TypeDef, TypeDefBody, UnaryOp,
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
    /// ジェネリック関数呼び出しの単相化情報。callee の span → (関数名, 型引数順の具体型)。
    /// コード生成が呼び出しごとの具体化記号を引くのに使う。型引数は外側の型パラメータを
    /// 含みうる（入れ子のジェネリック呼び出し。コード生成側で更に置換する）。
    pub mono: HashMap<Span, (String, Vec<Ty>)>,
    /// メソッド呼び出しの解決済み提供元。callee の span → トレイト名 or 受け手の型名。
    /// コード生成が同名衝突時に正しい記号（`@Type.Trait.method`）を選ぶのに使う。
    pub method_provider: HashMap<Span, String>,
    /// enum バリアント構築の情報。式の span → (enum型名, バリアントインデックス, has_payload)。
    /// コード生成が `Ident`/`Call` を enum 構築に変換するのに使う。
    pub variant_constructions: HashMap<Span, (String, usize, bool)>,
    /// 一般イテレータ `for x in it` の要素型。ループ変数の var_span → 要素型 `T`。
    /// コード生成が `next()` の戻り `Option<T>` をアンラップするのに使う。
    pub for_iter_elem: HashMap<Span, Ty>,
}

/// 型エラー。
#[derive(Debug, Clone)]
pub struct TypeError {
    pub span: Span,
    pub message: String,
}

/// 関数のシグネチャ。
struct FuncSig {
    /// 型パラメータ名と境界（`fn f<T: Bound>(...)`）。呼び出し時の単相化推論・境界検査に使う。
    generics: Vec<(String, Vec<Bound>)>,
    params: Vec<Ty>,
    ret: Ty,
}

/// トレイト境界 `Trait<Args>`（ADR-0006/0007）。`<T: Bound>`・スーパートレイトで使う。
#[derive(Clone)]
struct Bound {
    trait_name: String,
    args: Vec<Ty>,
}

/// トレイト定義の情報。メソッドのシグネチャは `Self`・トレイト型引数を含みうる。
struct TraitInfo {
    /// トレイトの型パラメータ名（`trait Iterator<T>` の `T`）。
    generics: Vec<String>,
    /// スーパートレイト（`trait Sub: Super`）。
    supertraits: Vec<Bound>,
    /// メソッド名 → シグネチャ。
    methods: HashMap<String, MethodSig>,
    /// 既定実装を持つメソッド名。
    defaults: HashSet<String>,
}

/// `impl Trait for Type` 1 件の情報。
struct TraitImpl {
    trait_name: String,
    trait_args: Vec<Ty>,
    /// この impl が明示的に提供したメソッド名 → (シグネチャ, 名前 span)。
    provided: HashMap<String, (MethodSig, Span)>,
    span: Span,
}

/// メソッド解決の提供元（固有メソッド or 実装トレイトの 1 件）。
struct Provider {
    sig: MethodSig,
    /// 曖昧時の表示・`#` 修飾子の一致に使う名前（トレイト名 or 型名）。
    label: String,
}

/// メソッドのシグネチャ。`params` は self を除いた引数の型。
#[derive(Clone)]
struct MethodSig {
    /// self の受け方。`None` は self なし（関連関数）。
    self_kind: Option<SelfKind>,
    params: Vec<Ty>,
    ret: Ty,
}

/// 型定義の情報。
enum TyDef {
    /// `type X = T`（名前的型付け）。元の型を持つ。
    Alias(Ty),
    /// `type X = struct { ... }`。型パラメータ名とフィールド（名前→型）。
    /// フィールド型は型パラメータ（ジェネリック struct）を含みうる。
    Struct {
        generics: Vec<String>,
        fields: Vec<(String, Ty)>,
    },
    /// `type X = enum { ... }`。型パラメータ名とバリアント一覧（名前, ペイロード型）を持つ。
    /// ペイロード型は型パラメータ（ジェネリック enum）を含みうる。
    Enum {
        generics: Vec<String>,
        variants: Vec<(String, Option<Ty>)>,
    },
}

/// プログラムの型検査を行う。
pub fn check(program: &Program, res: &Resolution) -> Result<TypeInfo, Vec<TypeError>> {
    let mut checker = Checker::new(program, res);
    for item in &program.items {
        match item {
            Item::Function(f) => checker.check_function(f),
            Item::Impl(im) => {
                checker.set_self_ty(Some(im.type_name.clone()));
                for m in &im.methods {
                    checker.check_function(m);
                }
                checker.set_self_ty(None);
            }
            // トレイトの既定実装の本体を検査する（`Self` は実装型を表す抽象型）。
            Item::Trait(tr) => {
                // Self はこのトレイトを実装する抽象型、トレイト型引数も抽象。
                let mut ambient: HashMap<String, Vec<Bound>> = HashMap::new();
                ambient.insert(
                    "Self".to_string(),
                    vec![Bound {
                        trait_name: tr.name.clone(),
                        args: tr.generics.iter().map(|g| Ty::named(&g.name)).collect(),
                    }],
                );
                for g in &tr.generics {
                    ambient.insert(g.name.clone(), Vec::new());
                }
                checker.ambient_generics = ambient;
                checker.set_self_ty(Some("Self".to_string()));
                for m in &tr.methods {
                    if m.default {
                        checker.check_function(&m.func);
                    }
                }
                checker.set_self_ty(None);
                checker.ambient_generics = HashMap::new();
            }
            Item::TypeDef(_) => {}
        }
    }
    // すべての `impl Trait for Type` の適合（conformance）を検査する。
    checker.check_conformance();
    if checker.errors.is_empty() {
        Ok(TypeInfo {
            expr_types: checker.expr_types,
            mono: checker.mono,
            method_provider: checker.method_provider,
            variant_constructions: checker.variant_constructions,
            for_iter_elem: checker.for_iter_elem,
        })
    } else {
        Err(checker.errors)
    }
}

struct Checker<'a> {
    res: &'a Resolution,
    funcs: HashMap<String, FuncSig>,
    /// 型名 → メソッド名 → シグネチャ（固有メソッド）。
    methods: HashMap<String, HashMap<String, MethodSig>>,
    /// トレイト名 → トレイト定義。
    traits: HashMap<String, TraitInfo>,
    /// 型名 → その型への `impl Trait for Type` 群。
    trait_impls: HashMap<String, Vec<TraitImpl>>,
    /// 現在検査中の関数の型パラメータ名 → 境界。`x.m()` を境界経由で解決するのに使う。
    generics: HashMap<String, Vec<Bound>>,
    /// 関数の外側から与える型パラメータ（トレイト既定実装の `Self`・トレイト型引数）。
    ambient_generics: HashMap<String, Vec<Bound>>,
    /// 現在の `Self` 型名（impl 対象の型、またはトレイト既定実装では `"Self"`）。
    self_ty: Option<String>,
    /// 型定義（別名・struct・enum）。
    types: HashMap<String, TyDef>,
    /// 既知の型名（プリミティブ＋組み込み＋定義済み）。型名検証に使う。
    known_types: HashSet<String>,
    /// DefId ごとの型。引数・ローカルの型を記録する。
    def_types: Vec<Ty>,
    /// 定義（宣言）位置の span から DefId を引くための表。
    def_spans: HashMap<Span, DefId>,
    expr_types: HashMap<Span, Ty>,
    /// ジェネリック関数呼び出しの単相化情報（callee span → (関数名, 型引数)）。
    mono: HashMap<Span, (String, Vec<Ty>)>,
    /// メソッド呼び出しの解決済み提供元（callee span → トレイト名 or 型名）。
    method_provider: HashMap<Span, String>,
    errors: Vec<TypeError>,
    /// 検査中の関数の戻り値型。
    current_ret: Ty,
    /// 現在ネストしているループの深さ（`break`/`continue` のループ外使用の検出に使う）。
    loop_depth: usize,
    /// バリアント名 → (enum型名, タグインデックス, ペイロード型)。非ジェネリック enum のみ。
    variant_owners: HashMap<String, (String, usize, Option<Ty>, Vec<String>)>,
    /// enum バリアント構築の記録（TypeInfo へ引き渡す）。
    variant_constructions: HashMap<Span, (String, usize, bool)>,
    /// `for x in it` の要素型（var_span → 要素型 T。TypeInfo へ引き渡す）。
    for_iter_elem: HashMap<Span, Ty>,
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
                let generics = f
                    .generics
                    .iter()
                    .map(|g| {
                        let bounds = g
                            .bounds
                            .iter()
                            .map(|b| Bound {
                                trait_name: b.name.clone(),
                                args: b.args.iter().map(Ty::from_ast).collect(),
                            })
                            .collect();
                        (g.name.clone(), bounds)
                    })
                    .collect();
                funcs.insert(
                    f.name.clone(),
                    FuncSig {
                        generics,
                        params,
                        ret,
                    },
                );
            }
        }

        // トレイト名も既知の型名として登録（境界・修飾子の名前検証用）。
        for item in &program.items {
            if let Item::Trait(t) = item {
                known_types.insert(t.name.clone());
            }
        }

        let mk_method_sig = |m: &Function| {
            // self_kind が Some のとき params[0] は合成 self なので飛ばす。
            let skip = usize::from(m.self_kind.is_some());
            let params = m.params[skip..].iter().map(|p| Ty::from_ast(&p.ty)).collect();
            let ret = m.ret.as_ref().map_or_else(Ty::unit, Ty::from_ast);
            MethodSig {
                self_kind: m.self_kind,
                params,
                ret,
            }
        };

        // トレイト定義テーブル。
        let mut traits: HashMap<String, TraitInfo> = HashMap::new();
        for item in &program.items {
            if let Item::Trait(t) = item {
                let mut tmethods = HashMap::new();
                let mut defaults = HashSet::new();
                for tm in &t.methods {
                    tmethods.insert(tm.func.name.clone(), mk_method_sig(&tm.func));
                    if tm.default {
                        defaults.insert(tm.func.name.clone());
                    }
                }
                traits.insert(
                    t.name.clone(),
                    TraitInfo {
                        generics: t.generics.iter().map(|g| g.name.clone()).collect(),
                        supertraits: t
                            .supertraits
                            .iter()
                            .map(|s| Bound {
                                trait_name: s.name.clone(),
                                args: s.args.iter().map(Ty::from_ast).collect(),
                            })
                            .collect(),
                        methods: tmethods,
                        defaults,
                    },
                );
            }
        }

        // 固有メソッドのシグネチャ（型名 → メソッド名 → sig）。self は params から除く。
        // 同じ型に同名メソッドがあれば（同一 impl・別 impl を問わず）二重定義として報告する。
        // trait impl のメソッドは固有表に入れず、trait_impls 側へ集める。
        let mut methods: HashMap<String, HashMap<String, MethodSig>> = HashMap::new();
        let mut trait_impls: HashMap<String, Vec<TraitImpl>> = HashMap::new();
        let mut dup_errors = Vec::new();
        for item in &program.items {
            let Item::Impl(im) = item else { continue };
            if let Some(tr) = &im.trait_ref {
                // `impl Trait for Type`：提供メソッドのシグネチャを記録する。
                let provided = im
                    .methods
                    .iter()
                    .map(|m| (m.name.clone(), (mk_method_sig(m), m.name_span)))
                    .collect();
                trait_impls.entry(im.type_name.clone()).or_default().push(TraitImpl {
                    trait_name: tr.name.clone(),
                    trait_args: tr.args.iter().map(Ty::from_ast).collect(),
                    provided,
                    span: tr.span,
                });
                continue;
            }
            // 固有 impl。
            let table = methods.entry(im.type_name.clone()).or_default();
            for m in &im.methods {
                if table.insert(m.name.clone(), mk_method_sig(m)).is_some() {
                    dup_errors.push(TypeError {
                        span: m.name_span,
                        message: format!(
                            "型 `{}` にメソッド `{}` が二重に定義されています",
                            im.type_name, m.name
                        ),
                    });
                }
            }
        }

        // 定義位置 span → DefId。ビルトインの span (0,0) は引かないため衝突しても無害。
        let def_spans = res
            .defs
            .iter()
            .enumerate()
            .map(|(id, def)| (def.span, id))
            .collect();

        // バリアント名 → (enum型名, タグ, ペイロード型, enum の型パラメータ名)。
        // ジェネリック enum も含む（ペイロード型は型パラメータを含みうる）。
        let mut variant_owners: HashMap<String, (String, usize, Option<Ty>, Vec<String>)> =
            HashMap::new();
        for item in &program.items {
            if let Item::TypeDef(t) = item
                && let TypeDefBody::Enum(variants) = &t.body
            {
                let generics: Vec<String> = t.generics.iter().map(|g| g.name.clone()).collect();
                for (idx, v) in variants.iter().enumerate() {
                    let payload_ty = v.payload.as_ref().map(Ty::from_ast);
                    variant_owners
                        .insert(v.name.clone(), (t.name.clone(), idx, payload_ty, generics.clone()));
                }
            }
        }

        Checker {
            res,
            funcs,
            methods,
            traits,
            trait_impls,
            generics: HashMap::new(),
            ambient_generics: HashMap::new(),
            self_ty: None,
            types,
            known_types,
            def_types: vec![Ty::Infer; res.defs.len()],
            def_spans,
            expr_types: HashMap::new(),
            mono: HashMap::new(),
            method_provider: HashMap::new(),
            errors: dup_errors,
            current_ret: Ty::unit(),
            loop_depth: 0,
            variant_owners,
            variant_constructions: HashMap::new(),
            for_iter_elem: HashMap::new(),
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
                // ジェネリック型パラメータ・`Self` は既知扱い。
                let is_self = name == "Self" && self.self_ty.is_some();
                if !is_self && !self.is_generic_param(name) && !self.known_types.contains(name) {
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
            // 匿名境界はパーサが脱糖済み。防御的に何もしない。
            Type::Bound { .. } => {}
        }
    }

    fn set_self_ty(&mut self, name: Option<String>) {
        self.self_ty = name;
    }

    fn check_function(&mut self, f: &Function) {
        // ジェネリック環境＝外側(ambient: Self・トレイト型引数) ＋ 関数自身の型パラメータ。
        let mut generics = self.ambient_generics.clone();
        for g in &f.generics {
            let bounds = g
                .bounds
                .iter()
                .map(|b| Bound {
                    trait_name: b.name.clone(),
                    args: b.args.iter().map(Ty::from_ast).collect(),
                })
                .collect();
            generics.insert(g.name.clone(), bounds);
        }
        self.generics = generics;

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
        // メソッドは `funcs` に無いので、関数自身の戻り値型から設定する。
        self.current_ret = f.ret.as_ref().map_or_else(Ty::unit, Ty::from_ast);
        self.check_block(&f.body);
        self.generics = HashMap::new();
    }

    /// 型名 `name` が現在のスコープのジェネリック型パラメータか。
    fn is_generic_param(&self, name: &str) -> bool {
        self.generics.contains_key(name)
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
                        // 注釈型から enum 構築・配列リテラルの未確定型を埋める。
                        self.refine_construction(value, &expected);
                        self.finalize_array_lit(value, &expected);
                        expected
                    }
                    // 注釈なしはリテラルを既定型へ確定する。
                    None => {
                        let d = value_ty.defaulted();
                        self.finalize_array_lit(value, &d);
                        d
                    }
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
                        // 戻り型から enum 構築・配列リテラルの未確定型を埋める。
                        self.refine_construction(v, &ret);
                        self.finalize_array_lit(v, &ret);
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
            Stmt::For {
                var_span,
                start,
                end,
                body,
                span,
                ..
            } => {
                let start_ty = self.check_expr(start);
                let end_ty = self.check_expr(end);
                // 範囲の境界は整数（現状 `for` は整数範囲のみ。浮動小数・一般の
                // イテレータは未対応）。
                if !matches!(start_ty, Ty::Error | Ty::Infer) && !self.is_integer_like(&start_ty) {
                    self.error(
                        start.span,
                        format!(
                            "`for` の範囲の下限は整数である必要があります（`{}`）",
                            start_ty.describe()
                        ),
                    );
                }
                if !matches!(end_ty, Ty::Error | Ty::Infer) && !self.is_integer_like(&end_ty) {
                    self.error(
                        end.span,
                        format!(
                            "`for` の範囲の上限は整数である必要があります（`{}`）",
                            end_ty.describe()
                        ),
                    );
                }
                // 下限と上限は同じ整数型であること（リテラルは具体型へ適合）。
                if !self.assignable(&start_ty, &end_ty) && !self.assignable(&end_ty, &start_ty) {
                    self.error(
                        *span,
                        format!(
                            "`for` の範囲の下限と上限の型が一致しません: `{}` と `{}`",
                            start_ty.describe(),
                            end_ty.describe()
                        ),
                    );
                }
                // ループ変数の型: 境界の具体整数型を優先し、両方リテラルなら既定 `i32`。
                let var_ty = if matches!(start_ty, Ty::Named { .. }) && self.is_integer_like(&start_ty) {
                    start_ty
                } else if matches!(end_ty, Ty::Named { .. }) && self.is_integer_like(&end_ty) {
                    end_ty
                } else {
                    Ty::named("i32")
                };
                if let Some(&id) = self.def_spans.get(var_span) {
                    self.def_types[id] = var_ty;
                }
                self.loop_depth += 1;
                self.check_block(body);
                self.loop_depth -= 1;
            }
            Stmt::ForIn {
                var_span,
                iter,
                body,
                span,
                ..
            } => {
                // 配列リテラルを直接反復する場合は既定（`T[]`）へ確定しておく。
                let iter_ty = self.check_expr(iter);
                self.finalize_array_lit(iter, &iter_ty.clone().defaulted());
                let iter_ty = self.expr_types.get(&iter.span).cloned().unwrap_or(iter_ty);
                // 配列 `T[]` / `Vec<T>` は要素を直接反復する。それ以外は `Iterator<T>` 実装を探す。
                let elem = self.sequence_elem(&iter_ty, *span);
                if let Some(&id) = self.def_spans.get(var_span) {
                    self.def_types[id] = elem.clone();
                }
                self.for_iter_elem.insert(*var_span, elem);
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
            ExprKind::Ident(name) => {
                // ユーザー定義 enum バリアント（ペイロードなし。ジェネリック含む）。
                if let Some((enum_name, tag, payload_ty, generics)) = self.variant_owners.get(name.as_str()).cloned() {
                    if let Some(&id) = self.res.uses.get(&expr.span)
                        && self.res.defs[id].kind == DefKind::Builtin
                    {
                        if payload_ty.is_some() {
                            self.error(
                                expr.span,
                                format!("バリアント `{name}` はペイロードが必要です（`{name}(value)` と書いてください）"),
                            );
                        }
                        self.variant_constructions.insert(expr.span, (enum_name.clone(), tag, false));
                        // ジェネリック enum はペイロードなしでは型引数を推論できないため Infer。
                        return Ty::Named {
                            name: enum_name.clone(),
                            args: vec![Ty::Infer; generics.len()],
                        };
                    }
                }
                // ビルトイン Option/Result コンストラクタ（ペイロードなし＝ `None`）。
                if let Some((enum_name, tag, false)) = prelude_variant(name) {
                    if let Some(&id) = self.res.uses.get(&expr.span)
                        && self.res.defs[id].kind == DefKind::Builtin
                    {
                        self.variant_constructions.insert(expr.span, (enum_name.into(), tag, false));
                        return builtin_ctor(name, &[]);
                    }
                }
                self.res
                    .uses
                    .get(&expr.span)
                    .map_or(Ty::Infer, |&id| self.def_types[id].clone())
            }
            ExprKind::Unary { op, expr: inner } => self.infer_unary(*op, inner),
            ExprKind::Binary { op, lhs, rhs } => self.infer_binary(*op, lhs, rhs),
            ExprKind::Call { callee, args } => self.infer_call(callee, args, expr.span),
            ExprKind::Member { object, field, .. } => self.infer_member(object, field, expr.span),
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
            ExprKind::EnumLit { payload, .. } => {
                // パーサは直接 EnumLit を出力しない（typeck 内部で記録するのみ）。
                // 防御的に payload を検査し Infer を返す。
                if let Some(p) = payload { self.check_expr(p); }
                Ty::Infer
            }
            ExprKind::Match { scrutinee, arms } => self.infer_match(scrutinee, arms, expr.span),
            ExprKind::ArrayLit { elems } => self.infer_array_lit(elems),
        }
    }

    /// 配列リテラル `[e1, e2, ...]` を型付けする。全要素を共通要素型へ `join` し、
    /// 未確定の配列リテラル型 `ArrayLit(elem)` を返す（`T[]`/`Vec<T>` への確定は
    /// 注釈に応じて [`Self::finalize_array_lit`] が行う）。
    fn infer_array_lit(&mut self, elems: &[Expr]) -> Ty {
        let mut elem: Option<Ty> = None;
        for e in elems {
            let et = self.check_expr(e);
            elem = Some(match elem {
                None => et,
                Some(prev) => self.join(&prev, &et).unwrap_or_else(|| {
                    self.error(
                        e.span,
                        format!(
                            "配列要素の型が一致しません: `{}` と `{}`",
                            prev.describe(),
                            et.describe()
                        ),
                    );
                    Ty::Error
                }),
            });
        }
        Ty::ArrayLit(Box::new(elem.unwrap_or(Ty::Infer)))
    }

    /// 配列リテラル式の型を、期待型（`T[]` または `Vec<T>`）に合わせて確定し、
    /// `expr_types` に記録し直す。要素のリテラル型も要素型へ確定する（codegen が
    /// 要素の幅を決められるように）。注釈が配列・Vec でなければ既定の `T[]` に確定する。
    fn finalize_array_lit(&mut self, expr: &Expr, target: &Ty) {
        let ExprKind::ArrayLit { elems } = &expr.kind else {
            return;
        };
        // 期待型から要素型と Vec か否かを取り出す。
        let (is_vec, elem_target) = match target.peel_refs() {
            Ty::Named { name, args } if name == "Vec" && args.len() == 1 => {
                (true, Some(args[0].clone()))
            }
            Ty::Array(e) => (false, Some((**e).clone())),
            _ => (false, None),
        };
        // 現在記録されている要素型（未確定でありうる）。
        let cur_elem = match self.expr_types.get(&expr.span) {
            Some(Ty::ArrayLit(e)) | Some(Ty::Array(e)) => Some((**e).clone()),
            Some(Ty::Named { name, args }) if name == "Vec" && args.len() == 1 => {
                Some(args[0].clone())
            }
            _ => None,
        };
        let vague = |t: &Ty| matches!(t, Ty::Infer | Ty::IntLit | Ty::FloatLit);
        let melem = match (elem_target, cur_elem) {
            (Some(t), Some(c)) => if vague(&c) && !vague(&t) { t } else { c },
            (Some(t), None) => t,
            (None, Some(c)) => c.defaulted(),
            (None, None) => Ty::Infer,
        };
        let resolved = if is_vec {
            Ty::Named { name: "Vec".to_string(), args: vec![melem.clone()] }
        } else {
            Ty::Array(Box::new(melem.clone()))
        };
        self.expr_types.insert(expr.span, resolved);
        // 要素の未確定リテラル型を要素型へ確定し、入れ子の配列リテラルも辿る。
        for e in elems {
            if !vague(&melem)
                && matches!(self.expr_types.get(&e.span), Some(t) if vague(t))
            {
                self.expr_types.insert(e.span, melem.clone());
            }
            self.finalize_array_lit(e, &melem);
        }
    }

    fn infer_match(&mut self, scrutinee: &Expr, arms: &[MatchArm], span: Span) -> Ty {
        let scrut_ty = self.check_expr(scrutinee).defaulted();

        // scrutinee が enum 型なら、バリアント情報を取り出す。ビルトイン Option/Result は
        // 型引数を具体ペイロードに反映して合成する（束縛変数へ具体型を付けるため）。
        let enum_variants: Option<Vec<(String, Option<Ty>)>> = match &scrut_ty {
            Ty::Named { name, args } if name == "Option" => {
                let t = args.first().cloned().unwrap_or(Ty::Infer);
                Some(vec![("None".into(), None), ("Some".into(), Some(t))])
            }
            Ty::Named { name, args } if name == "Result" => {
                let t = args.first().cloned().unwrap_or(Ty::Infer);
                let e = args.get(1).cloned().unwrap_or(Ty::Infer);
                Some(vec![("Ok".into(), Some(t)), ("Err".into(), Some(e))])
            }
            Ty::Named { name, args } => match self.types.get(name) {
                Some(TyDef::Enum { generics, variants }) => {
                    // ジェネリック enum は scrutinee の型引数でペイロードを単相化する。
                    let mut subst_map: HashMap<String, Ty> = HashMap::new();
                    for (g, a) in generics.iter().zip(args) {
                        subst_map.insert(g.clone(), a.clone());
                    }
                    Some(
                        variants
                            .iter()
                            .map(|(n, p)| (n.clone(), p.as_ref().map(|t| subst(t, &subst_map))))
                            .collect(),
                    )
                }
                _ => None,
            },
            _ => None,
        };
        let enum_name: Option<String> = match &scrut_ty {
            Ty::Named { name, .. } if enum_variants.is_some() => Some(name.clone()),
            _ => None,
        };

        let mut result_ty: Ty = Ty::Infer;

        for arm in arms {
            // パターン検証。
            match &arm.pattern {
                Pattern::Wildcard { .. } => {}
                Pattern::Lit { value, span: pat_span } => {
                    // リテラルパターンの型が scrutinee に適合するか確認。
                    let pat_ty = match value {
                        LitPat::Int(_) => Ty::IntLit,
                        LitPat::Float(_) => Ty::FloatLit,
                        LitPat::Bool(_) => Ty::named("bool"),
                        LitPat::Str(_) => Ty::named("string"),
                    };
                    if !self.assignable(&scrut_ty, &pat_ty) && scrut_ty != Ty::Infer && scrut_ty != Ty::Error {
                        self.error(
                            *pat_span,
                            format!(
                                "パターンの型 `{}` が scrutinee の型 `{}` と一致しません",
                                pat_ty.describe(),
                                scrut_ty.describe()
                            ),
                        );
                    }
                }
                Pattern::Range { lo, hi, span: pat_span, .. } => {
                    // 範囲の境界は数値リテラル（整数または浮動小数）。
                    let bound_ty = |b: &LitPat| match b {
                        LitPat::Int(_) => Ty::IntLit,
                        LitPat::Float(_) => Ty::FloatLit,
                        _ => Ty::Error,
                    };
                    let lo_ty = bound_ty(lo);
                    let hi_ty = bound_ty(hi);
                    // 下限と上限は同じ数値クラスであること。
                    if lo_ty != hi_ty {
                        self.error(
                            *pat_span,
                            "範囲パターンの下限と上限は同じ数値型である必要があります".to_string(),
                        );
                    } else if !self.assignable(&scrut_ty, &lo_ty)
                        && scrut_ty != Ty::Infer
                        && scrut_ty != Ty::Error
                    {
                        self.error(
                            *pat_span,
                            format!(
                                "範囲パターンの型 `{}` が scrutinee の型 `{}` と一致しません",
                                lo_ty.describe(),
                                scrut_ty.describe()
                            ),
                        );
                    }
                }
                Pattern::Variant { name, binding, span: pat_span } => {
                    if let Some(ref evs) = enum_variants {
                        // バリアントが enum に存在するか確認。
                        let found = evs.iter().find(|(vn, _)| vn == name);
                        match found {
                            None => {
                                self.error(
                                    *pat_span,
                                    format!(
                                        "`{}` は `{}` のバリアントではありません",
                                        name,
                                        enum_name.as_deref().unwrap_or("?")
                                    ),
                                );
                            }
                            Some((_, payload_ty)) => {
                                // 束縛 x の型を登録。
                                if let Some((bname, bspan)) = binding {
                                    let bty = payload_ty.clone().unwrap_or(Ty::Infer);
                                    if let Some(&id) = self.def_spans.get(bspan) {
                                        self.def_types[id] = bty;
                                    }
                                } else if payload_ty.is_some() {
                                    self.error(
                                        *pat_span,
                                        format!(
                                            "バリアント `{name}` はペイロードを持ちます（`{name}(binding)` と書いてください）"
                                        ),
                                    );
                                }
                            }
                        }
                    } else {
                        // 非 enum に対するバリアントパターン（OK の場合も Infer 扱い）。
                        // 束縛があれば Infer 型として登録する。
                        if let Some((_, bspan)) = binding {
                            if let Some(&id) = self.def_spans.get(bspan) {
                                self.def_types[id] = Ty::Infer;
                            }
                        }
                    }
                }
            }

            // ガードは bool でなければならない（束縛変数の型は上で登録済み）。
            if let Some(g) = &arm.guard {
                let gt = self.check_expr(g);
                self.expect_bool(&gt, g.span, "match ガード");
            }

            let arm_ty = self.check_expr(&arm.body).defaulted();
            // 全アームの型を合流させる（Infer は無視、最初の具体型を採用）。
            if result_ty == Ty::Infer || result_ty == Ty::Error {
                result_ty = arm_ty;
            } else if arm_ty != Ty::Infer && arm_ty != Ty::Error && arm_ty != result_ty {
                // 型の不一致は警告程度（エラーにするとテストが壊れる）。
                // 一致させようとするが、合わなければエラーのみ記録して Infer に戻す。
                if !self.assignable(&result_ty, &arm_ty) {
                    self.error(
                        arm.body.span,
                        format!(
                            "match アームの型が一致しません: `{}` を期待しましたが `{}` でした",
                            result_ty.describe(),
                            arm_ty.describe()
                        ),
                    );
                    result_ty = Ty::Error;
                }
            }
        }

        if result_ty == Ty::Infer {
            Ty::unit()
        } else {
            result_ty
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

    fn infer_call(&mut self, callee: &Expr, args: &[Expr], call_span: Span) -> Ty {
        let arg_tys: Vec<Ty> = args.iter().map(|a| self.check_expr(a)).collect();

        // メソッド呼び出し `object.method(args)`（callee がメンバアクセス）。
        if let ExprKind::Member {
            object,
            field,
            qualifier,
        } = &callee.kind
        {
            return self.infer_method_call(
                object,
                field,
                qualifier.as_ref(),
                callee.span,
                args,
                &arg_tys,
            );
        }

        if let ExprKind::Ident(name) = &callee.kind
            && let Some(&id) = self.res.uses.get(&callee.span)
        {
            // ユーザー定義 enum バリアント（ペイロードあり。ジェネリック含む）。
            if self.res.defs[id].kind == DefKind::Builtin {
                if let Some((enum_name, tag, payload_ty, generics)) = self.variant_owners.get(name).cloned() {
                    // ジェネリック enum はペイロード引数から型パラメータを推論する。
                    let gset: HashSet<&str> = generics.iter().map(|s| s.as_str()).collect();
                    let mut subst_map: HashMap<String, Ty> = HashMap::new();
                    // ペイロード型検査。
                    let payload_expr = args.first();
                    if let Some(pt) = &payload_ty {
                        if args.len() != 1 {
                            self.error(
                                callee.span,
                                format!(
                                    "バリアント `{name}` のペイロードは 1 個ですが {} 個渡されました",
                                    args.len()
                                ),
                            );
                        } else if let Some(a) = payload_expr {
                            let at = self.check_expr(a);
                            unify(pt, &at, &gset, &mut subst_map);
                            let expected = subst(pt, &subst_map);
                            if !self.assignable(&expected, &at) {
                                self.error(
                                    a.span,
                                    format!(
                                        "バリアント `{name}` のペイロードの型が一致しません: `{}` を期待しましたが `{}` でした",
                                        expected.describe(),
                                        at.describe()
                                    ),
                                );
                            }
                        }
                    } else if !args.is_empty() {
                        self.error(
                            callee.span,
                            format!("バリアント `{name}` はペイロードを持ちません"),
                        );
                    }
                    // バリアント構築として記録（Call 式全体の span に記録）。
                    self.variant_constructions.insert(call_span, (enum_name.clone(), tag, true));
                    // 推論した型パラメータで enum の型引数を埋める（未束縛は Infer）。
                    let enum_args: Vec<Ty> = generics
                        .iter()
                        .map(|g| subst_map.get(g).cloned().unwrap_or(Ty::Infer))
                        .collect();
                    return Ty::Named { name: enum_name.clone(), args: enum_args };
                }
                // ビルトイン Option/Result コンストラクタ（ペイロードあり）。
                if let Some((enum_name, tag, _)) = prelude_variant(name) {
                    self.variant_constructions.insert(call_span, (enum_name.into(), tag, true));
                }
                return builtin_ctor(name, &arg_tys);
            }
            match self.res.defs[id].kind {
                DefKind::Function => return self.check_func_call(name, callee.span, args, &arg_tys),
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
        let generics = sig.generics.clone();
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

        // 単相化推論: 型パラメータ → 引数から推論した具体型。
        let gset: HashSet<&str> = generics.iter().map(|(n, _)| n.as_str()).collect();
        let mut smap: HashMap<String, Ty> = HashMap::new();
        if !gset.is_empty() {
            for (p, a) in params.iter().zip(arg_tys) {
                unify(p, a, &gset, &mut smap);
            }
        }

        for ((expected, actual), arg) in params.iter().zip(arg_tys).zip(args) {
            let exp = if gset.is_empty() {
                expected.clone()
            } else {
                subst(expected, &smap)
            };
            if !self.assignable(&exp, actual) {
                self.error(
                    arg.span,
                    format!(
                        "引数の型が一致しません: `{}` を期待しましたが `{}` でした",
                        exp.describe(),
                        actual.describe()
                    ),
                );
            }
        }

        // トレイト境界の充足検査: 各型パラメータの推論結果が境界トレイトを実装しているか。
        for (gname, bounds) in &generics {
            if bounds.is_empty() {
                continue;
            }
            let Some(concrete) = smap.get(gname) else {
                continue;
            };
            let Ty::Named { name: cname, .. } = concrete.peel_refs() else {
                continue;
            };
            // 別のジェネリック型パラメータへ束縛された場合は単相化時に再検査される。
            if self.is_generic_param(cname) {
                continue;
            }
            for b in bounds {
                if !self.type_implements(cname, &b.trait_name, &b.args) {
                    self.error(
                        span,
                        format!(
                            "`{cname}` はトレイト境界 `{}` を満たしていません（`impl {} for {cname}` が必要）",
                            b.trait_name, b.trait_name
                        ),
                    );
                }
            }
        }

        if gset.is_empty() {
            ret
        } else {
            // 単相化情報を記録（型引数を宣言順に並べる）。
            let type_args: Vec<Ty> = generics
                .iter()
                .map(|(n, _)| smap.get(n).cloned().unwrap_or(Ty::Infer))
                .collect();
            self.mono.insert(span, (name.to_string(), type_args));
            subst(&ret, &smap)
        }
    }

    /// メソッド呼び出し `object.method(args)` を静的ディスパッチで型付けする（ADR-0004）。
    /// 受け手が具象型なら固有＋実装トレイトのメソッド、ジェネリック型パラメータなら境界
    /// （スーパートレイト含む）から提供元を集め、ちょうど 1 個へ解決する。複数なら `#` で明示。
    fn infer_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        qualifier: Option<&TraitRef>,
        span: Span,
        args: &[Expr],
        arg_tys: &[Ty],
    ) -> Ty {
        let obj_ty = self.check_expr(object);
        // 受け手の型名（参照は剥がす）。
        let Ty::Named { name: ty_name, args: ty_args } = obj_ty.peel_refs().clone() else {
            if !matches!(obj_ty, Ty::Infer | Ty::Error) {
                self.error(
                    span,
                    format!("型 `{}` にメソッドはありません", obj_ty.describe()),
                );
            }
            return Ty::Infer;
        };

        // 提供元を集める。label は曖昧時の表示と `#` 修飾子の一致に使う。
        let recv = Ty::Named { name: ty_name.clone(), args: ty_args };
        let mut providers = self.collect_providers(&ty_name, &recv, method);

        // 修飾子 `#Qualifier` があれば提供元を絞る。
        if let Some(q) = qualifier {
            providers.retain(|p| p.label == q.name);
        }

        let sig = match providers.len() {
            1 => {
                let p = providers.pop().unwrap();
                // コード生成のため、解決した提供元（トレイト名 or 型名）を記録する。
                self.method_provider.insert(span, p.label);
                p.sig
            }
            0 => {
                if let Some(q) = qualifier {
                    self.error(
                        span,
                        format!("`{ty_name}` には `{}` 由来のメソッド `{method}` はありません", q.name),
                    );
                } else {
                    self.error(
                        span,
                        format!("型 `{ty_name}` にメソッド `{method}` はありません"),
                    );
                }
                return Ty::Error;
            }
            _ => {
                let labels: Vec<_> = providers
                    .iter()
                    .map(|p| format!("`{method}#{}`", p.label))
                    .collect();
                self.error(
                    span,
                    format!(
                        "メソッド `{method}` の提供元が複数あります。{} のように `#` で明示してください",
                        labels.join(" / ")
                    ),
                );
                return providers.pop().unwrap().sig.ret;
            }
        };

        // self を取らない関連関数は、値からのドット呼び出しでは呼べない。
        if sig.self_kind.is_none() {
            self.error(
                span,
                format!("`{method}` は self を取らないため `値.{method}()` で呼べません"),
            );
        }
        // `&mut self` は可変な受け手を要する。
        if sig.self_kind == Some(SelfKind::RefMut) {
            self.check_mut_receiver(object, &obj_ty, method);
        }

        if sig.params.len() != arg_tys.len() {
            self.error(
                span,
                format!(
                    "メソッド `{method}` の引数は {} 個ですが {} 個渡されました",
                    sig.params.len(),
                    arg_tys.len()
                ),
            );
            return sig.ret;
        }
        for ((expected, actual), arg) in sig.params.iter().zip(arg_tys).zip(args) {
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
        sig.ret
    }

    /// メソッド `method` の提供元を集める。具象型では固有＋実装トレイトの直接メソッド、
    /// ジェネリック型パラメータでは境界（スーパートレイトを辿る）から集める。
    fn collect_providers(&self, ty_name: &str, recv: &Ty, method: &str) -> Vec<Provider> {
        let mut providers = Vec::new();
        if self.is_generic_param(ty_name) {
            for bound in self.generics.get(ty_name).cloned().unwrap_or_default() {
                if let Some((owner, sig)) =
                    self.resolve_trait_method(&bound.trait_name, &bound.args, recv, method)
                {
                    // ラベルはメソッドを実際に定義するトレイト（スーパートレイト由来なら親）。
                    providers.push(Provider { sig, label: owner });
                }
            }
            return providers;
        }
        // 具象型：固有メソッド。
        if let Some(sig) = self.methods.get(ty_name).and_then(|m| m.get(method)) {
            providers.push(Provider {
                sig: sig.clone(),
                label: ty_name.to_string(),
            });
        }
        // 実装トレイト（直接のメソッドのみ。スーパートレイト由来はそのトレイト自身の impl が提供）。
        if let Some(impls) = self.trait_impls.get(ty_name) {
            for ti in impls {
                if let Some(tinfo) = self.traits.get(&ti.trait_name)
                    && let Some(sig) = tinfo.methods.get(method)
                {
                    let mut map = HashMap::new();
                    map.insert("Self".to_string(), recv.clone());
                    for (g, a) in tinfo.generics.iter().zip(&ti.trait_args) {
                        map.insert(g.clone(), a.clone());
                    }
                    providers.push(Provider {
                        sig: subst_sig(sig, &map),
                        label: ti.trait_name.clone(),
                    });
                }
            }
        }
        providers
    }

    /// トレイト `trait_name<trait_args>` とそのスーパートレイトから `method` を探し、
    /// `(メソッドを定義するトレイト名, Self・型引数を置換した具体シグネチャ)` を返す。
    fn resolve_trait_method(
        &self,
        trait_name: &str,
        trait_args: &[Ty],
        recv: &Ty,
        method: &str,
    ) -> Option<(String, MethodSig)> {
        let tinfo = self.traits.get(trait_name)?;
        let mut map = HashMap::new();
        map.insert("Self".to_string(), recv.clone());
        for (g, a) in tinfo.generics.iter().zip(trait_args) {
            map.insert(g.clone(), a.clone());
        }
        if let Some(sig) = tinfo.methods.get(method) {
            return Some((trait_name.to_string(), subst_sig(sig, &map)));
        }
        for s in &tinfo.supertraits {
            let s_args: Vec<Ty> = s.args.iter().map(|a| subst(a, &map)).collect();
            if let Some(found) = self.resolve_trait_method(&s.trait_name, &s_args, recv, method) {
                return Some(found);
            }
        }
        None
    }

    /// 具象型 `ty_name` がトレイト `trait_name<args>` を実装しているか（可視範囲・ADR-0008）。
    fn type_implements(&self, ty_name: &str, trait_name: &str, args: &[Ty]) -> bool {
        self.trait_impls.get(ty_name).is_some_and(|impls| {
            impls.iter().any(|ti| {
                ti.trait_name == trait_name
                    && ti.trait_args.len() == args.len()
                    && ti
                        .trait_args
                        .iter()
                        .zip(args)
                        .all(|(x, y)| self.tys_match(x, y))
            })
        })
    }

    /// `for x in coll` の対象型から要素型を求める。配列 `T[]` / `Vec<T>` は要素 `T` を
    /// 直接取り出す（当面は要素が Copy 型のときのみ。借用反復・値束縛）。それ以外は
    /// `Iterator<T>` 実装を探す（[`Self::iterator_elem`]）。
    fn sequence_elem(&mut self, coll_ty: &Ty, span: Span) -> Ty {
        let elem = match coll_ty.peel_refs() {
            Ty::Array(e) | Ty::ArrayLit(e) => Some((**e).clone().defaulted()),
            Ty::Named { name, args } if name == "Vec" && args.len() == 1 => {
                Some(args[0].clone().defaulted())
            }
            _ => None,
        };
        match elem {
            Some(e) => {
                if !matches!(e, Ty::Infer | Ty::Error) && !is_copy_ty(&e) {
                    self.error(
                        span,
                        format!(
                            "`for ... in` は Copy 型の要素のみ反復できます（要素型 `{}` は Copy ではありません）",
                            e.describe()
                        ),
                    );
                }
                e
            }
            None => self.iterator_elem(coll_ty, span),
        }
    }

    /// `for x in it` の対象型から、実装する `Iterator<T>` の要素型 T を求める（ADR-0007）。
    /// `Iterator` を実装していなければエラーを記録し `Ty::Error` を返す。複数の `Iterator`
    /// 実装（`Iterator<i32>`/`Iterator<string>` 同居）は当面曖昧エラー（`for x#T in` 未実装）。
    fn iterator_elem(&mut self, iter_ty: &Ty, span: Span) -> Ty {
        if matches!(iter_ty, Ty::Infer | Ty::Error) {
            return Ty::Infer;
        }
        let Ty::Named { name: ty_name, .. } = iter_ty.peel_refs().clone() else {
            self.error(
                span,
                format!("`for ... in` の対象 `{}` はイテレータではありません", iter_ty.describe()),
            );
            return Ty::Error;
        };
        let elems: Vec<Ty> = self
            .trait_impls
            .get(&ty_name)
            .map(|impls| {
                impls
                    .iter()
                    .filter(|ti| ti.trait_name == "Iterator")
                    .filter_map(|ti| ti.trait_args.first().cloned())
                    .collect()
            })
            .unwrap_or_default();
        match elems.len() {
            1 => elems.into_iter().next().unwrap(),
            0 => {
                self.error(
                    span,
                    format!("型 `{ty_name}` は `Iterator` を実装していないため `for ... in` で反復できません"),
                );
                Ty::Error
            }
            _ => {
                self.error(
                    span,
                    format!("型 `{ty_name}` は複数の `Iterator` 実装を持つため要素型が曖昧です"),
                );
                Ty::Error
            }
        }
    }

    /// 双方向に適合する型同士か（トレイト引数の一致判定。Infer はワイルドカード）。
    fn tys_match(&self, a: &Ty, b: &Ty) -> bool {
        self.assignable(a, b) || self.assignable(b, a)
    }

    /// すべての `impl Trait for Type` の適合を検査する（ADR-0004/0007/0008）。
    /// - トレイトの全メソッドを提供（既定実装があれば省略可）し、シグネチャが一致する
    /// - トレイトに無いメソッドを置かない
    /// - スーパートレイトの実装が存在する
    /// - 同一キー `(Trait<引数>, Type)` の重複実装を弾く（スコープ・コヒーレンス）
    fn check_conformance(&mut self) {
        let mut errs: Vec<TypeError> = Vec::new();
        for (ty_name, impls) in &self.trait_impls {
            for (idx, ti) in impls.iter().enumerate() {
                // 重複実装（同一キー）。`#` でも選べないため弾く。
                for other in &impls[..idx] {
                    if other.trait_name == ti.trait_name
                        && other.trait_args.len() == ti.trait_args.len()
                        && other
                            .trait_args
                            .iter()
                            .zip(&ti.trait_args)
                            .all(|(x, y)| self.tys_match(x, y))
                    {
                        errs.push(TypeError {
                            span: ti.span,
                            message: format!(
                                "`{}` への `{}` の実装が重複しています（同一スコープで一意でなければなりません）",
                                ty_name, ti.trait_name
                            ),
                        });
                    }
                }

                let Some(tinfo) = self.traits.get(&ti.trait_name) else {
                    errs.push(TypeError {
                        span: ti.span,
                        message: format!("未定義のトレイト `{}`", ti.trait_name),
                    });
                    continue;
                };

                if ti.trait_args.len() != tinfo.generics.len() {
                    errs.push(TypeError {
                        span: ti.span,
                        message: format!(
                            "トレイト `{}` の型引数は {} 個ですが {} 個指定されています",
                            ti.trait_name,
                            tinfo.generics.len(),
                            ti.trait_args.len()
                        ),
                    });
                }

                // 置換マップ: Self → 実装型, トレイト型引数 → impl の指定。
                let recv = Ty::named(ty_name);
                let mut map = HashMap::new();
                map.insert("Self".to_string(), recv);
                for (g, a) in tinfo.generics.iter().zip(&ti.trait_args) {
                    map.insert(g.clone(), a.clone());
                }

                // 各トレイトメソッドの提供・シグネチャ一致。
                for (mname, tsig) in &tinfo.methods {
                    let expected = subst_sig(tsig, &map);
                    match ti.provided.get(mname) {
                        Some((isig, ispan)) => {
                            let mut ok = isig.self_kind == expected.self_kind
                                && isig.params.len() == expected.params.len()
                                && isig
                                    .params
                                    .iter()
                                    .zip(&expected.params)
                                    .all(|(a, b)| self.tys_match(a, b))
                                && self.tys_match(&isig.ret, &expected.ret);
                            // 既定実装の self を上書きしている場合の不一致も拾う。
                            if !ok {
                                errs.push(TypeError {
                                    span: *ispan,
                                    message: format!(
                                        "メソッド `{mname}` のシグネチャがトレイト `{}` と一致しません",
                                        ti.trait_name
                                    ),
                                });
                                ok = true; // 二重報告を避ける
                            }
                            let _ = ok;
                        }
                        None => {
                            if !tinfo.defaults.contains(mname) {
                                errs.push(TypeError {
                                    span: ti.span,
                                    message: format!(
                                        "`{}` への `{}` の実装にメソッド `{mname}` がありません",
                                        ty_name, ti.trait_name
                                    ),
                                });
                            }
                        }
                    }
                }

                // トレイトに存在しないメソッドを置いていないか。
                for (mname, (_, ispan)) in &ti.provided {
                    if !tinfo.methods.contains_key(mname) {
                        errs.push(TypeError {
                            span: *ispan,
                            message: format!(
                                "トレイト `{}` にメソッド `{mname}` はありません",
                                ti.trait_name
                            ),
                        });
                    }
                }

                // スーパートレイトの実装が存在するか（ADR-0007）。
                for s in &tinfo.supertraits {
                    let s_args: Vec<Ty> = s.args.iter().map(|a| subst(a, &map)).collect();
                    if !self.type_implements(ty_name, &s.trait_name, &s_args) {
                        errs.push(TypeError {
                            span: ti.span,
                            message: format!(
                                "`{}` は `{}` の前提となるスーパートレイト `{}` を実装していません",
                                ty_name, ti.trait_name, s.trait_name
                            ),
                        });
                    }
                }
            }
        }
        self.errors.extend(errs);
    }

    /// `&mut self` メソッドの受け手が可変か検査する。不変参照越し・不変束縛は不可。
    fn check_mut_receiver(&mut self, object: &Expr, obj_ty: &Ty, method: &str) {
        // 受け手が不変参照 `&T` のとき、そこから `&mut self` は取れない。
        if let Ty::Ref { mutable: false, .. } = obj_ty {
            self.error(
                object.span,
                format!("不変参照からは可変メソッド `{method}` を呼べません（`&mut` が必要）"),
            );
            return;
        }
        // 受け手が値で、不変な束縛のときは可変借用できない。
        if let ExprKind::Ident(name) = &object.kind
            && let Some(&id) = self.res.uses.get(&object.span)
        {
            let def = &self.res.defs[id];
            if def.kind == DefKind::Local && !def.mutable {
                self.error(
                    object.span,
                    format!(
                        "不変な変数 `{name}` では可変メソッド `{method}` を呼べません（`let mut` が必要）"
                    ),
                );
            }
        }
    }

    /// メンバアクセス `object.field` の型を求める。
    fn infer_member(&mut self, object: &Expr, field: &str, span: Span) -> Ty {
        let mut obj = self.check_expr(object);
        // 参照は自動でたどる。
        while let Ty::Ref { inner, .. } = obj {
            obj = *inner;
        }
        if let Ty::Named { name, args } = &obj
            && let Some((generics, fields)) = self.struct_fields(name)
        {
            return match fields.iter().find(|(n, _)| n == field) {
                // ジェネリック struct はインスタンスの型引数でフィールド型を単相化する。
                Some((_, fty)) => {
                    let mut map = HashMap::new();
                    for (g, a) in generics.iter().zip(args) {
                        map.insert(g.clone(), a.clone());
                    }
                    subst(fty, &map)
                }
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

        // ジェネリック struct は各フィールドの値型から型パラメータを推論する（unify）。
        let gset: HashSet<&str> = generics.iter().map(|s| s.as_str()).collect();
        let mut subst_map: HashMap<String, Ty> = HashMap::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for fi in fields {
            let vty = self.check_expr(&fi.value);
            if !seen.insert(&fi.name) {
                self.error(fi.name_span, format!("フィールド `{}` が重複しています", fi.name));
                continue;
            }
            match def_fields.iter().find(|(n, _)| n == &fi.name) {
                Some((_, fty)) => {
                    // 型パラメータを値型から推論し、置換後の期待型と照合する。
                    unify(fty, &vty, &gset, &mut subst_map);
                    let expected = subst(fty, &subst_map);
                    if !self.assignable(&expected, &vty) {
                        self.error(
                            fi.value.span,
                            format!(
                                "フィールド `{}` の型が一致しません: `{}` を期待しましたが `{}` でした",
                                fi.name,
                                expected.describe(),
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

        // 推論した型パラメータで型引数を埋める（未束縛は Infer、リテラルは既定型へ確定）。
        let args: Vec<Ty> = generics
            .iter()
            .map(|g| subst_map.get(g).cloned().unwrap_or(Ty::Infer).defaulted())
            .collect();
        Ty::Named {
            name: name.to_string(),
            args,
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

    /// enum 構築式の未確定な型引数を、期待型から埋めて `expr_types` を更新する。
    /// `return Err(Fail { .. })` のように成功側 `Ok` の型が文脈からしか決まらない場合、
    /// codegen の per-instantiation レイアウト（集約ペイロード）が文脈と一致するようにする。
    fn refine_construction(&mut self, expr: &Expr, expected: &Ty) {
        let Some(recorded) = self.expr_types.get(&expr.span).cloned() else {
            return;
        };
        if let (Ty::Named { name: rn, args: ra }, Ty::Named { name: en, args: ea }) =
            (&recorded, expected)
            && rn == en
            && ra.len() == ea.len()
        {
            let vague = |t: &Ty| matches!(t, Ty::Infer | Ty::IntLit | Ty::FloatLit);
            let merged_args: Vec<Ty> = ra
                .iter()
                .zip(ea)
                .map(|(r, e)| if vague(r) && !vague(e) { e.clone() } else { r.clone() })
                .collect();
            self.expr_types.insert(
                expr.span,
                Ty::Named { name: rn.clone(), args: merged_args },
            );
        }
    }

    fn infer_try(&mut self, inner: &Expr) -> Ty {
        let t = self.check_expr(inner);
        // 現在の関数が Result/Option を返さなければ `!` は使えない。
        let ret_known = !matches!(&self.current_ret, Ty::Infer | Ty::Error);
        if ret_known && !is_result_or_option(&self.current_ret) {
            self.error(
                inner.span,
                "`!` は Result または Option を返す関数の中でのみ使えます".to_string(),
            );
        }
        match &t {
            Ty::Infer | Ty::Error => Ty::Infer,
            Ty::Named { name, args } if (name == "Result" || name == "Option") => {
                // inner の種別（Result/Option）が関数の戻り型の種別と一致すること。
                // 失敗時、Result は同じ E を伝播し、Option は None を伝播するため。
                if let Ty::Named { name: ret_name, args: ret_args } = &self.current_ret
                    && (ret_name == "Result" || ret_name == "Option")
                {
                    if ret_name != name {
                        self.error(
                            inner.span,
                            format!(
                                "`!` で伝播できません: 関数は `{ret_name}` を返しますが `{name}` に使っています"
                            ),
                        );
                    } else if name == "Result" {
                        // Err の型 E は戻り型の E と互換でなければならない。
                        let inner_e = args.get(1).cloned().unwrap_or(Ty::Infer);
                        let ret_e = ret_args.get(1).cloned().unwrap_or(Ty::Infer);
                        if !self.assignable(&ret_e, &inner_e) {
                            self.error(
                                inner.span,
                                format!(
                                    "`!` のエラー型が一致しません: 関数の `Err` は `{}` ですが `{}` を伝播しようとしています",
                                    ret_e.describe(),
                                    inner_e.describe()
                                ),
                            );
                        }
                    }
                }
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
    fn struct_fields(&self, name: &str) -> Option<(Vec<String>, Vec<(String, Ty)>)> {
        let mut cur = name.to_string();
        for _ in 0..32 {
            match self.types.get(&cur)? {
                TyDef::Struct { generics, fields } => {
                    return Some((generics.clone(), fields.clone()));
                }
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
            // 配列リテラルは固定長配列・Vec のどちらの注釈にも適合する（型指向）。
            (Array(i1), ArrayLit(i2)) | (ArrayLit(i1), Array(i2)) | (ArrayLit(i1), ArrayLit(i2)) => {
                self.assignable(i1, i2)
            }
            (Named { name, args }, ArrayLit(i2)) if name == "Vec" && args.len() == 1 => {
                self.assignable(&args[0], i2)
            }
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

/// 型パラメータ名を具体型へ置換する（`Self`・トレイト型引数の単相化）。
fn subst(ty: &Ty, map: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::Named { name, args } => {
            if args.is_empty()
                && let Some(rep) = map.get(name)
            {
                return rep.clone();
            }
            Ty::Named {
                name: name.clone(),
                args: args.iter().map(|a| subst(a, map)).collect(),
            }
        }
        Ty::Ref { mutable, inner } => Ty::Ref {
            mutable: *mutable,
            inner: Box::new(subst(inner, map)),
        },
        Ty::Array(i) => Ty::Array(Box::new(subst(i, map))),
        Ty::ArrayLit(i) => Ty::ArrayLit(Box::new(subst(i, map))),
        Ty::Tuple(es) => Ty::Tuple(es.iter().map(|e| subst(e, map)).collect()),
        other => other.clone(),
    }
}

/// 型が Copy（ムーブせずコピー）か（`for ... in` の要素束縛の制限に使う）。
/// 数値・bool・char・不変参照・要素が全て Copy のタプルが Copy。未確定/不明は Copy 扱い。
fn is_copy_ty(ty: &Ty) -> bool {
    match ty {
        Ty::IntLit | Ty::FloatLit | Ty::Infer | Ty::Error => true,
        Ty::Ref { mutable, .. } => !*mutable,
        Ty::Tuple(elems) => elems.iter().all(is_copy_ty),
        Ty::Named { name, args } if args.is_empty() => {
            INT_TYPES.contains(&name.as_str())
                || FLOAT_TYPES.contains(&name.as_str())
                || name == "bool"
                || name == "char"
        }
        _ => false,
    }
}

/// 単相化推論: パラメータ型 `param`（型パラメータ `gset` を含みうる）と実引数型 `arg` を
/// 突き合わせ、型パラメータ → 具体型の束縛を `out` に集める。最初の束縛を優先する。
fn unify(param: &Ty, arg: &Ty, gset: &HashSet<&str>, out: &mut HashMap<String, Ty>) {
    match (param, arg) {
        (Ty::Named { name, args }, _) if args.is_empty() && gset.contains(name.as_str()) => {
            // 参照は剥がし、リテラルは既定型へ確定して束縛する。
            let bound = arg.peel_refs().clone().defaulted();
            out.entry(name.clone()).or_insert(bound);
        }
        (Ty::Named { args: pa, .. }, Ty::Named { args: aa, .. }) if pa.len() == aa.len() => {
            for (p, a) in pa.iter().zip(aa) {
                unify(p, a, gset, out);
            }
        }
        (Ty::Ref { inner: pi, .. }, Ty::Ref { inner: ai, .. }) => unify(pi, ai, gset, out),
        // 暗黙デリファレンス: `&T` 引数を値パラメータへ。
        (Ty::Named { .. }, Ty::Ref { inner: ai, .. }) => unify(param, ai, gset, out),
        (Ty::Array(p), Ty::Array(a))
        | (Ty::Array(p), Ty::ArrayLit(a))
        | (Ty::ArrayLit(p), Ty::Array(a))
        | (Ty::ArrayLit(p), Ty::ArrayLit(a)) => unify(p, a, gset, out),
        // Vec<T> パラメータへ配列リテラル引数: 要素型を突き合わせる。
        (Ty::Named { name, args }, Ty::ArrayLit(a)) if name == "Vec" && args.len() == 1 => {
            unify(&args[0], a, gset, out)
        }
        (Ty::Tuple(ps), Ty::Tuple(es)) if ps.len() == es.len() => {
            for (p, a) in ps.iter().zip(es) {
                unify(p, a, gset, out);
            }
        }
        _ => {}
    }
}

/// メソッドシグネチャの引数・戻り値型を置換する。
fn subst_sig(sig: &MethodSig, map: &HashMap<String, Ty>) -> MethodSig {
    MethodSig {
        self_kind: sig.self_kind,
        params: sig.params.iter().map(|p| subst(p, map)).collect(),
        ret: subst(&sig.ret, map),
    }
}

/// AST の型定義を内部表現へ変換する。
fn lower_type_def(t: &TypeDef) -> TyDef {
    match &t.body {
        TypeDefBody::Alias(ty) => TyDef::Alias(Ty::from_ast(ty)),
        TypeDefBody::Struct(fields) => TyDef::Struct {
            generics: t.generics.iter().map(|g| g.name.clone()).collect(),
            fields: fields
                .iter()
                .map(|f| (f.name.clone(), Ty::from_ast(&f.ty)))
                .collect(),
        },
        TypeDefBody::Enum(variants) => TyDef::Enum {
            generics: t.generics.iter().map(|g| g.name.clone()).collect(),
            variants: variants
                .iter()
                .map(|v| (v.name.clone(), v.payload.as_ref().map(Ty::from_ast)))
                .collect(),
        },
    }
}

/// プレリュードの Option/Result コンストラクタ名を (enum名, タグ, ペイロード有無) に対応づける。
/// codegen のバリアント構築記録に使う。タグは codegen の `StructReg.enum_layouts` と一致させる。
fn prelude_variant(name: &str) -> Option<(&'static str, usize, bool)> {
    match name {
        "None" => Some(("Option", 0, false)),
        "Some" => Some(("Option", 1, true)),
        "Ok" => Some(("Result", 0, true)),
        "Err" => Some(("Result", 1, true)),
        _ => None,
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
