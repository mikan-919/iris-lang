//! LLVM IR コード生成（テキスト `.ll` を出力）。
//!
//! 型検査・所有権検査を通った AST を LLVM IR のテキストへ落とす。生成した IR は
//! `clang file.ll -o out` で実行ファイルにできる（このリポジトリの環境には
//! `llvm-config` が無く inkwell/llvm-sys が使えないため、まずはテキスト出力とする）。
//!
//! 対応範囲（数値プリミティブと `bool`）:
//! - 関数定義・引数・再帰呼び出し
//! - `let` / 再代入 / `return`
//! - 算術 `+ - * / %`、比較、論理 `&& ||`（短絡）、単項 `-`
//! - `if` 文・三項演算子（基本ブロックで分岐）
//! - 整数 `i8..u64`（符号付き/なしで命令を選ぶ）と浮動小数 `f32`/`f64`
//! - 参照 `&T` / `&mut T`（opaque ポインタ。`&x` は場所のアドレス。値の文脈では
//!   暗黙にデリファレンス＝`load` する）
//! - struct（名前付き LLVM 構造体型）。構造体リテラル `Name { ... }`・メンバアクセス
//!   `a.b`・フィールドへの代入 `a.b = v`。値は first-class な構造体値として扱う
//!
//! - 文字列 `string`（C 風の **NUL 終端**表現。リテラルは `[N x i8]` のグローバル定数
//!   `@.str.N` にし、文字列値は opaque ポインタ `ptr` として扱う。`extern fn puts` 等で
//!   libc に渡して出力できる。長さ・索引・連結などの操作はまだ無い）
//!
//! ローカルは alloca + load/store で扱う（SSA 化は LLVM の mem2reg に任せられる）。
//! enum・`!`・ジェネリクスなどは未対応（エラーにする）。参照越しの代入
//! （write-through `r = v`、`r: &mut T`）は参照値（`ptr`）の指す先へ `store` する。
//! 右辺が参照型のときは束縛の付け替え（rebind）になる。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::ast::{
    BinaryOp, Block, Else, Expr, ExprKind, FieldInit, Function, Item, LitPat, MatchArm, Pattern,
    Program, SelfKind, Stmt, Type, TypeDefBody, UnaryOp,
};
use crate::sema::resolve::{DefId, Resolution};
use crate::sema::ty::Ty;
use crate::span::Span;

/// コード生成エラー。
#[derive(Debug, Clone)]
pub struct CodegenError {
    pub span: Option<Span>,
    pub message: String,
}

impl CodegenError {
    fn new(span: Span, message: impl Into<String>) -> Self {
        CodegenError {
            span: Some(span),
            message: message.into(),
        }
    }
}

/// struct のレイアウト情報（コード生成で参照する）。
///
/// 非ジェネリックな struct を名前 → フィールド列（宣言順）で持つ。`type X = Y` の
/// 別名は `alias_to` に記録し、struct 解決・LLVM 型変換のとき末尾までたどる。
#[derive(Default)]
struct StructReg {
    /// struct 名 → フィールド（名前, 型）の宣言順リスト。
    layouts: HashMap<String, Vec<(String, Ty)>>,
    /// 別名 `type A = B`（B は引数なし名前付き型）の A → B。
    alias_to: HashMap<String, String>,
}

impl StructReg {
    /// プログラムの型定義から struct レイアウトと別名を集める。
    fn build(program: &Program) -> StructReg {
        let mut reg = StructReg::default();
        for item in &program.items {
            let Item::TypeDef(t) = item else { continue };
            match &t.body {
                // ジェネリックな struct は単一化未実装のためコード生成では扱わない。
                TypeDefBody::Struct(fields) if t.generics.is_empty() => {
                    let layout = fields
                        .iter()
                        .map(|f| (f.name.clone(), Ty::from_ast(&f.ty)))
                        .collect();
                    reg.layouts.insert(t.name.clone(), layout);
                }
                // `type A = B`（引数なし名前付き型）は別名としてたどれるようにする。
                TypeDefBody::Alias(Type::Named { name, args, .. }) if args.is_empty() => {
                    reg.alias_to.insert(t.name.clone(), name.clone());
                }
                _ => {}
            }
        }
        reg
    }

    /// 名前を別名チェーンでたどり、struct なら（正規名, フィールド列）を返す。
    fn struct_def(&self, name: &str) -> Option<(String, &Vec<(String, Ty)>)> {
        let mut cur = name.to_string();
        for _ in 0..32 {
            if let Some(fields) = self.layouts.get(&cur) {
                return Some((cur, fields));
            }
            match self.alias_to.get(&cur) {
                Some(t) => cur = t.clone(),
                None => return None,
            }
        }
        None
    }
}

/// enum のレイアウト情報。
///
/// 非ジェネリックな enum を名前 → バリアント列（宣言順）で持つ。
/// LLVM 表現は `{ i8, i64 }` 固定（タグ＋ペイロードを i64 に格納）。
#[derive(Default)]
struct EnumReg {
    /// enum 名 → バリアント（名前, ペイロード型）の宣言順リスト。
    layouts: HashMap<String, Vec<(String, Option<Ty>)>>,
}

impl EnumReg {
    fn build(program: &Program) -> EnumReg {
        let mut reg = EnumReg::default();
        for item in &program.items {
            let Item::TypeDef(t) = item else { continue };
            if let TypeDefBody::Enum(variants) = &t.body
                && t.generics.is_empty()
            {
                let layout = variants
                    .iter()
                    .map(|v| (v.name.clone(), v.payload.as_ref().map(|p| Ty::from_ast(p))))
                    .collect();
                reg.layouts.insert(t.name.clone(), layout);
            }
        }
        reg
    }

    /// バリアント名からタグインデックスを返す。
    fn tag_of(&self, enum_name: &str, variant: &str) -> Option<usize> {
        let vs = self.layouts.get(enum_name)?;
        vs.iter().position(|(n, _)| n == variant)
    }

    /// enum 名 → バリアント一覧を返す。
    fn variants(&self, enum_name: &str) -> Option<&Vec<(String, Option<Ty>)>> {
        self.layouts.get(enum_name)
    }
}

/// 文字列リテラルのグローバル定数プール。
///
/// 文字列は C 風の **NUL 終端**表現を採る（文字列値 = `[N x i8]` 定数へのポインタ）。
/// `intern` で内容ごとに `@.str.N` を割り当て、定義行を貯めておき、モジュール末尾へ
/// まとめて出力する（LLVM IR ではグローバル定義の順序は問わない）。
#[derive(Default)]
struct StringPool {
    defs: Vec<String>,
}

impl StringPool {
    /// 文字列の内容にグローバル定数を割り当て、その記号（`@.str.N`）を返す。
    /// opaque ポインタなので記号はそのまま `ptr` 値（先頭バイトのアドレス）に使える。
    fn intern(&mut self, content: &str) -> String {
        let id = self.defs.len();
        let name = format!("@.str.{id}");
        let (encoded, len) = encode_cstr(content);
        self.defs.push(format!(
            "{name} = private unnamed_addr constant [{len} x i8] c\"{encoded}\""
        ));
        name
    }
}

/// 文字列を LLVM IR の `c"..."` 表記へ符号化し、(符号化済み文字列, バイト長) を返す。
/// 印字可能 ASCII 以外と `"` `\` は `\XX`（16進）でエスケープし、末尾に NUL を付ける。
/// マルチバイト UTF-8 はバイト単位でエスケープされる。
fn encode_cstr(s: &str) -> (String, usize) {
    let mut out = String::new();
    let mut len = 0;
    for &b in s.as_bytes() {
        len += 1;
        if b == b'"' || b == b'\\' || !(0x20..=0x7e).contains(&b) {
            let _ = write!(out, "\\{b:02X}");
        } else {
            out.push(b as char);
        }
    }
    out.push_str("\\00");
    len += 1;
    (out, len)
}

/// プログラム全体を LLVM IR のテキストへ変換する。
pub fn emit_module(
    program: &Program,
    res: &Resolution,
    type_info: &crate::sema::TypeInfo,
) -> Result<String, CodegenError> {
    let def_spans: HashMap<Span, DefId> = res
        .defs
        .iter()
        .enumerate()
        .map(|(id, d)| (d.span, id))
        .collect();

    // 関数名 → 定義（呼び出しのシグネチャ解決用）。
    let mut func_table: HashMap<String, &Function> = HashMap::new();
    for item in &program.items {
        if let Item::Function(f) = item {
            func_table.insert(f.name.clone(), f);
        }
    }

    // トレイト名 → トレイト定義（既定実装の合成に使う）。
    let mut trait_table: HashMap<String, &crate::ast::TraitDef> = HashMap::new();
    for item in &program.items {
        if let Item::Trait(t) = item {
            trait_table.insert(t.name.clone(), t);
        }
    }

    // (型名, メソッド名, 提供元ラベル) → メソッド定義。提供元ラベルは固有メソッドでは
    // 型名、トレイト実装/既定実装ではトレイト名。同名衝突時に記号を分けるための鍵。
    // 固有/トレイト impl のメソッドに加え、上書きされていないトレイト既定実装も登録する。
    let mut method_defs: HashMap<(String, String, String), &Function> = HashMap::new();
    // (型名, メソッド名) → 提供元ラベル集合。要素 2 個以上なら同名衝突（記号を分ける）。
    let mut method_labels: HashMap<(String, String), HashSet<String>> = HashMap::new();
    for item in &program.items {
        if let Item::Impl(im) = item {
            // 提供元ラベル: 固有 impl は型名、`impl Trait for` はトレイト名。
            let label = im
                .trait_ref
                .as_ref()
                .map_or_else(|| im.type_name.clone(), |t| t.name.clone());
            for m in &im.methods {
                method_defs.insert(
                    (im.type_name.clone(), m.name.clone(), label.clone()),
                    m,
                );
                method_labels
                    .entry((im.type_name.clone(), m.name.clone()))
                    .or_default()
                    .insert(label.clone());
            }
            // `impl Trait for Type` の未提供の既定実装をこの型のメソッドとして登録。
            if let Some(tr) = &im.trait_ref
                && let Some(tdef) = trait_table.get(&tr.name)
            {
                let provided: HashSet<&str> =
                    im.methods.iter().map(|m| m.name.as_str()).collect();
                for tm in &tdef.methods {
                    if tm.default && !provided.contains(tm.func.name.as_str()) {
                        method_defs.insert(
                            (im.type_name.clone(), tm.func.name.clone(), label.clone()),
                            &tm.func,
                        );
                        method_labels
                            .entry((im.type_name.clone(), tm.func.name.clone()))
                            .or_default()
                            .insert(label.clone());
                    }
                }
            }
        }
    }
    // 同名衝突した (型名, メソッド名) の集合。
    let method_collisions: HashSet<(String, String)> = method_labels
        .iter()
        .filter(|(_, labels)| labels.len() > 1)
        .map(|(k, _)| k.clone())
        .collect();

    let structs = StructReg::build(program);

    let mut module = String::from("; iris-lang が生成した LLVM IR\n\n");

    // 名前付き struct 型を宣言する（`%Name = type { ... }`）。プログラム順で
    // 出力して再現性を保つ。型宣言は前方参照が許されるので順序は問わない。
    for item in &program.items {
        if let Item::TypeDef(t) = item
            && let TypeDefBody::Struct(fields) = &t.body
            && t.generics.is_empty()
        {
            let mut field_tys = Vec::new();
            for f in fields {
                let ty = Ty::from_ast(&f.ty);
                field_tys.push(
                    llvm_ty(&ty, &structs).map_err(|m| CodegenError::new(f.span, m))?,
                );
            }
            let _ = writeln!(module, "%{} = type {{ {} }}", t.name, field_tys.join(", "));
        }
    }
    if !structs.layouts.is_empty() {
        module.push('\n');
    }

    // 非ジェネリック enum の型宣言 `%Name = type { i8, i64 }` を出力する。
    let enums = EnumReg::build(program);
    let mut enum_decls_emitted = false;
    for item in &program.items {
        if let Item::TypeDef(t) = item
            && let TypeDefBody::Enum(_) = &t.body
            && t.generics.is_empty()
        {
            let _ = writeln!(module, "%{} = type {{ i8, i64 }}", t.name);
            enum_decls_emitted = true;
        }
    }
    if enum_decls_emitted {
        module.push('\n');
    }

    // 文字列リテラルのグローバル定数はここに集め、関数生成後に末尾へ出力する。
    let strings = RefCell::new(StringPool::default());

    // 関数 / メソッドをまとめて出力する。メソッドの記号は `Type.method` に変える。
    // `subst` は単相化中の型パラメータ束縛（非ジェネリックでは空）。
    let empty_subst: HashMap<String, Ty> = HashMap::new();
    let mut emit_one = |f: &Function,
                        symbol: &str,
                        subst: &HashMap<String, Ty>|
     -> Result<(), CodegenError> {
        let mut cg = FnCodegen {
            res,
            types: &type_info.expr_types,
            def_spans: &def_spans,
            func_table: &func_table,
            method_defs: &method_defs,
            method_collisions: &method_collisions,
            method_provider: &type_info.method_provider,
            structs: &structs,
            enums: &enums,
            variant_constructions: &type_info.variant_constructions,
            strings: &strings,
            body: String::new(),
            tmp: 0,
            label: 0,
            locals: HashMap::new(),
            terminated: false,
            loops: Vec::new(),
            ret_ty: Ty::unit(),
            type_subst: subst,
            mono: &type_info.mono,
        };
        let func_ir = cg.emit_function(f, symbol)?;
        module.push_str(&func_ir);
        module.push('\n');
        Ok(())
    };

    // ジェネリック関数のテンプレートはここでは出さず、単相化して必要な実体だけを出す。
    let generic_funcs: HashMap<&str, &Function> = program
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Function(f) if !f.generics.is_empty() => Some((f.name.as_str(), f)),
            _ => None,
        })
        .collect();

    // 非ジェネリック関数・全 impl メソッド・未上書きの既定実装を出力する。
    // あわせて、各本体に現れるジェネリック呼び出しを単相化の種として集める。
    let mut seen: HashSet<String> = HashSet::new();
    let mut worklist: Vec<(String, Vec<Ty>)> = Vec::new();
    let seed = |body: &Block, subst: &HashMap<String, Ty>, seen: &mut HashSet<String>, wl: &mut Vec<(String, Vec<Ty>)>| {
        let mut spans = Vec::new();
        collect_call_callees(body, &mut spans);
        for s in spans {
            if let Some((gname, gargs)) = type_info.mono.get(&s) {
                let concrete: Vec<Ty> = gargs.iter().map(|a| subst_ty(a, subst)).collect();
                let sym = mono_symbol(gname, &concrete);
                if seen.insert(sym) {
                    wl.push((gname.clone(), concrete));
                }
            }
        }
    };

    for item in &program.items {
        match item {
            Item::Function(f) if f.generics.is_empty() => {
                seed(&f.body, &empty_subst, &mut seen, &mut worklist);
                emit_one(f, &f.name, &empty_subst)?;
            }
            // ジェネリックテンプレートは単相化時にのみ出す。
            Item::Function(_) => {}
            Item::Impl(im) => {
                let label = im
                    .trait_ref
                    .as_ref()
                    .map_or_else(|| im.type_name.clone(), |t| t.name.clone());
                for m in &im.methods {
                    let colliding =
                        method_collisions.contains(&(im.type_name.clone(), m.name.clone()));
                    let symbol = method_symbol(&im.type_name, &m.name, &label, colliding);
                    seed(&m.body, &empty_subst, &mut seen, &mut worklist);
                    emit_one(m, &symbol, &empty_subst)?;
                }
                // 未上書きの既定実装を合成する（Self → 実装型）。
                if let Some(tr) = &im.trait_ref
                    && let Some(tdef) = trait_table.get(&tr.name)
                {
                    let provided: HashSet<&str> =
                        im.methods.iter().map(|m| m.name.as_str()).collect();
                    let mut dsubst: HashMap<String, Ty> = HashMap::new();
                    dsubst.insert("Self".to_string(), Ty::named(&im.type_name));
                    for (g, a) in tdef.generics.iter().zip(&tr.args) {
                        dsubst.insert(g.name.clone(), Ty::from_ast(a));
                    }
                    for tm in &tdef.methods {
                        if tm.default && !provided.contains(tm.func.name.as_str()) {
                            let colliding = method_collisions
                                .contains(&(im.type_name.clone(), tm.func.name.clone()));
                            let symbol =
                                method_symbol(&im.type_name, &tm.func.name, &label, colliding);
                            seed(&tm.func.body, &dsubst, &mut seen, &mut worklist);
                            emit_one(&tm.func, &symbol, &dsubst)?;
                        }
                    }
                }
            }
            // トレイト定義そのものは IR に出さない（既定実装は impl ごとに合成済み）。
            Item::Trait(_) => {}
            // 型定義はコード生成では型情報としてのみ使い、IR には出さない。
            Item::TypeDef(_) => {}
        }
    }

    // 単相化ワークリストを処理する（入れ子のジェネリック呼び出しも閉包に含める）。
    while let Some((name, args)) = worklist.pop() {
        let Some(f) = generic_funcs.get(name.as_str()) else {
            continue;
        };
        let mut subst: HashMap<String, Ty> = HashMap::new();
        for (g, a) in f.generics.iter().zip(&args) {
            subst.insert(g.name.clone(), a.clone());
        }
        seed(&f.body, &subst, &mut seen, &mut worklist);
        let symbol = mono_symbol(&name, &args);
        emit_one(f, &symbol, &subst)?;
    }

    // 文字列リテラルのグローバル定数を末尾にまとめて出力する。
    let pool = strings.into_inner();
    if !pool.defs.is_empty() {
        module.push('\n');
        for d in &pool.defs {
            module.push_str(d);
            module.push('\n');
        }
    }

    Ok(module)
}

struct FnCodegen<'a> {
    res: &'a Resolution,
    types: &'a HashMap<Span, Ty>,
    def_spans: &'a HashMap<Span, DefId>,
    func_table: &'a HashMap<String, &'a Function>,
    /// (型名, メソッド名, 提供元ラベル) → メソッド定義。
    method_defs: &'a HashMap<(String, String, String), &'a Function>,
    /// 同名衝突した (型名, メソッド名)。記号を提供元で分ける必要があるもの。
    method_collisions: &'a HashSet<(String, String)>,
    /// メソッド呼び出しの解決済み提供元（typeck が記録。callee span → ラベル）。
    method_provider: &'a HashMap<Span, String>,
    structs: &'a StructReg,
    enums: &'a EnumReg,
    /// バリアント構築の情報（typeck が記録）。
    variant_constructions: &'a HashMap<Span, (String, usize, bool)>,
    /// 文字列リテラルのグローバル定数プール（全関数で共有）。
    strings: &'a RefCell<StringPool>,
    body: String,
    tmp: usize,
    label: usize,
    /// 束縛 DefId → (ポインタレジスタ, LLVM 型)。
    locals: HashMap<DefId, (String, String)>,
    terminated: bool,
    /// ネスト中のループの (continue 先ラベル, break 先ラベル) のスタック。
    loops: Vec<(String, String)>,
    /// 現在の関数の戻り値の内部型（`return` での暗黙 deref 判定に使う）。
    ret_ty: Ty,
    /// 単相化中の型パラメータ束縛（`T` → 具体型）。具体化された関数本体で使う。
    /// 空なら非ジェネリック関数。
    type_subst: &'a HashMap<String, Ty>,
    /// 単相化情報（callee span → (関数名, 型引数)）。ジェネリック呼び出しの記号解決に使う。
    mono: &'a HashMap<Span, (String, Vec<Ty>)>,
}

/// 型パラメータ名を具体型へ置換する（単相化）。
fn subst_ty(ty: &Ty, map: &HashMap<String, Ty>) -> Ty {
    if map.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::Named { name, args } => {
            if args.is_empty()
                && let Some(rep) = map.get(name)
            {
                return rep.clone();
            }
            Ty::Named {
                name: name.clone(),
                args: args.iter().map(|a| subst_ty(a, map)).collect(),
            }
        }
        Ty::Ref { mutable, inner } => Ty::Ref {
            mutable: *mutable,
            inner: Box::new(subst_ty(inner, map)),
        },
        Ty::Array(i) => Ty::Array(Box::new(subst_ty(i, map))),
        Ty::Tuple(es) => Ty::Tuple(es.iter().map(|e| subst_ty(e, map)).collect()),
        other => other.clone(),
    }
}

/// ブロック中のすべての関数呼び出しの callee span を集める（単相化の探索用）。
fn collect_call_callees(block: &Block, out: &mut Vec<Span>) {
    for s in &block.stmts {
        walk_stmt_calls(s, out);
    }
}

fn walk_stmt_calls(stmt: &Stmt, out: &mut Vec<Span>) {
    match stmt {
        Stmt::Let { value, .. } => walk_expr_calls(value, out),
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                walk_expr_calls(v, out);
            }
        }
        Stmt::Assign { target, value, .. } => {
            walk_expr_calls(target, out);
            walk_expr_calls(value, out);
        }
        Stmt::While { cond, body, .. } => {
            walk_expr_calls(cond, out);
            collect_call_callees(body, out);
        }
        Stmt::Loop { body, .. } => collect_call_callees(body, out),
        Stmt::For { start, end, body, .. } => {
            walk_expr_calls(start, out);
            walk_expr_calls(end, out);
            collect_call_callees(body, out);
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Expr(e) => walk_expr_calls(e, out),
    }
}

fn walk_expr_calls(expr: &Expr, out: &mut Vec<Span>) {
    match &expr.kind {
        ExprKind::Unary { expr: e, .. } => walk_expr_calls(e, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr_calls(lhs, out);
            walk_expr_calls(rhs, out);
        }
        ExprKind::Call { callee, args } => {
            out.push(callee.span);
            walk_expr_calls(callee, out);
            for a in args {
                walk_expr_calls(a, out);
            }
        }
        ExprKind::Member { object, .. } => walk_expr_calls(object, out),
        ExprKind::Ternary {
            cond,
            then,
            otherwise,
        } => {
            walk_expr_calls(cond, out);
            walk_expr_calls(then, out);
            walk_expr_calls(otherwise, out);
        }
        ExprKind::Try(e) => walk_expr_calls(e, out),
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            walk_expr_calls(cond, out);
            collect_call_callees(then, out);
            if let Some(els) = otherwise {
                match els.as_ref() {
                    Else::If(e) => walk_expr_calls(e, out),
                    Else::Block(b) => collect_call_callees(b, out),
                }
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for f in fields {
                walk_expr_calls(&f.value, out);
            }
        }
        ExprKind::EnumLit { payload, .. } => {
            if let Some(p) = payload {
                walk_expr_calls(p, out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_calls(scrutinee, out);
            for arm in arms {
                walk_expr_calls(&arm.body, out);
            }
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_) => {}
    }
}

/// メソッドの LLVM 記号を作る。同名衝突が無ければ `Type.method`、衝突時は固有メソッドは
/// `Type.method`、トレイトメソッドは `Type.Trait.method` で一意化する（ADR-0004 の `#`）。
/// `label` は提供元（固有なら型名 = `type_name`、トレイトならトレイト名）。
fn method_symbol(type_name: &str, method: &str, label: &str, colliding: bool) -> String {
    if !colliding || label == type_name {
        format!("{type_name}.{method}")
    } else {
        format!("{type_name}.{label}.{method}")
    }
}

/// 単相化の記号を作る（`greet_all` × `[Point]` → `greet_all.Point`）。
fn mono_symbol(name: &str, args: &[Ty]) -> String {
    let mut s = name.to_string();
    for a in args {
        s.push('.');
        s.push_str(&mangle_ty(a));
    }
    s
}

/// 型を記号に使える名前へ変換する。
fn mangle_ty(ty: &Ty) -> String {
    match ty {
        Ty::Named { name, args } if args.is_empty() => name.clone(),
        Ty::Named { name, args } => {
            let inner: Vec<_> = args.iter().map(mangle_ty).collect();
            format!("{name}_{}", inner.join("_"))
        }
        Ty::Ref { inner, .. } => format!("ref_{}", mangle_ty(inner)),
        Ty::Array(i) => format!("arr_{}", mangle_ty(i)),
        Ty::Tuple(es) => {
            let inner: Vec<_> = es.iter().map(mangle_ty).collect();
            format!("tup_{}", inner.join("_"))
        }
        other => other.describe(),
    }
}

impl<'a> FnCodegen<'a> {
    /// AST の型注釈を、単相化置換を適用して内部型へ変換する。
    fn lower(&self, t: &Type) -> Ty {
        subst_ty(&Ty::from_ast(t), self.type_subst)
    }
    /// 関数（メソッド）を生成する。`symbol` は LLVM の関数記号（メソッドは
    /// `Type.method`）。self は `f.params` の先頭に合成済みなので通常の引数として扱う。
    fn emit_function(&mut self, f: &Function, symbol: &str) -> Result<String, CodegenError> {
        self.ret_ty = match &f.ret {
            Some(t) => self.lower(t),
            None => Ty::unit(),
        };
        let ret_ty = llvm_ty(&self.ret_ty, self.structs)
            .map_err(|m| CodegenError::new(f.name_span, m))?;

        // extern は C 関数の宣言だけを出す。
        if f.is_extern {
            let mut tys = Vec::new();
            for p in &f.params {
                tys.push(
                    llvm_ty(&self.lower(&p.ty), self.structs).map_err(|m| CodegenError::new(p.span, m))?,
                );
            }
            return Ok(format!("declare {ret_ty} @{}({})\n", symbol, tys.join(", ")));
        }

        // 引数リスト。
        let mut params_sig = Vec::new();
        let mut param_setup = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            let ty = llvm_ty(&self.lower(&p.ty), self.structs)
                .map_err(|m| CodegenError::new(p.span, m))?;
            params_sig.push(format!("{ty} %arg{i}"));
            // 引数を alloca に退避して、ローカルと同様に扱う。
            if let Some(&id) = self.def_spans.get(&p.span) {
                let ptr = format!("%{}.addr", p.name);
                param_setup.push(format!("  {ptr} = alloca {ty}\n"));
                param_setup.push(format!("  store {ty} %arg{i}, ptr {ptr}\n"));
                self.locals.insert(id, (ptr, ty));
            }
        }

        let stmts = &f.body.stmts;
        // 末尾式の暗黙 return 判定: 最後の stmt が Stmt::Expr で関数が非 void 返却型のとき、
        // その値を ret に使う。それ以外は gen_stmt に任せる。
        let tail_expr = if ret_ty != "void" {
            if let Some(Stmt::Expr(e)) = stmts.last() { Some(e) } else { None }
        } else {
            None
        };
        let non_tail = if tail_expr.is_some() { &stmts[..stmts.len() - 1] } else { stmts.as_slice() };
        for stmt in non_tail {
            self.gen_stmt(stmt)?;
        }

        // 末尾式があればその値で ret、なければ既定の return を補う。
        if !self.terminated {
            if let Some(e) = tail_expr {
                let (v, _) = self.gen_value(e, &self.ret_ty.clone())?;
                // gen_value 内でブロックが終端している場合、または値が空（if 文など）は ret を出さない。
                if !self.terminated && !v.is_empty() {
                    self.emit(&format!("ret {ret_ty} {v}"));
                    self.terminated = true;
                }
            } else if ret_ty == "void" {
                self.emit("ret void");
            } else {
                self.emit(&format!("ret {ret_ty} {}", zero_value(&ret_ty)));
            }
        }

        let mut out = String::new();
        let _ = writeln!(
            out,
            "define {ret_ty} @{}({}) {{",
            symbol,
            params_sig.join(", ")
        );
        out.push_str("entry:\n");
        for s in &param_setup {
            out.push_str(s);
        }
        out.push_str(&self.body);
        out.push_str("}\n");
        Ok(out)
    }

    // ---- 文 -------------------------------------------------------------

    fn gen_stmt(&mut self, stmt: &Stmt) -> Result<(), CodegenError> {
        // 終端済みブロックの後ろは到達不能。新しいブロックを開く。
        if self.terminated {
            let l = self.fresh_label("dead");
            self.emit_label(&l);
        }
        match stmt {
            Stmt::Let {
                ty, value, span, ..
            } => {
                // 注釈があればその型、無ければ値の型（参照はそのまま束縛する）。
                let want = match ty {
                    Some(t) => self.lower(t),
                    None => self.raw_ty(value).defaulted(),
                };
                let (v, llty) = self.gen_value(value, &want)?;
                if let Some(&id) = self.def_spans.get(span) {
                    let ptr = format!("%{}.slot{}", "v", id);
                    self.emit(&format!("{ptr} = alloca {llty}"));
                    self.emit(&format!("store {llty} {v}, ptr {ptr}"));
                    self.locals.insert(id, (ptr, llty));
                }
                Ok(())
            }
            Stmt::Return { value, .. } => {
                match value {
                    Some(v) => {
                        let want = self.ret_ty.clone();
                        let (r, llty) = self.gen_value(v, &want)?;
                        self.emit(&format!("ret {llty} {r}"));
                    }
                    None => self.emit("ret void"),
                }
                self.terminated = true;
                Ok(())
            }
            Stmt::Assign { target, value, .. } => {
                let target_ty = self.raw_ty(target).defaulted();
                let value_ty = self.raw_ty(value).defaulted();
                // 参照越し代入（write-through）: 代入先が参照型で右辺が値型のとき、
                // target を参照値（`ptr`）として評価し、その参照先へ store する。
                if let Ty::Ref { inner, .. } = &target_ty
                    && !matches!(value_ty, Ty::Ref { .. })
                {
                    let dest = self.gen_expr(target, "ptr")?;
                    let inner_ty = (**inner).clone();
                    let (v, vllty) = self.gen_value(value, &inner_ty)?;
                    self.emit(&format!("store {vllty} {v}, ptr {dest}"));
                } else {
                    let (ptr, llty) = self.place_ptr(target)?;
                    let (v, _) = self.gen_value(value, &target_ty)?;
                    self.emit(&format!("store {llty} {v}, ptr {ptr}"));
                }
                Ok(())
            }
            Stmt::While { cond, body, .. } => self.gen_while(cond, body),
            Stmt::Loop { body, .. } => self.gen_loop(body),
            Stmt::For {
                var_span,
                start,
                end,
                inclusive,
                body,
                ..
            } => self.gen_for(var_span, start, end, *inclusive, body),
            Stmt::Break { span } => {
                let (_, brk) = self
                    .loops
                    .last()
                    .ok_or_else(|| CodegenError::new(*span, "`break` がループの外にあります"))?;
                self.emit(&format!("br label %{brk}"));
                self.terminated = true;
                Ok(())
            }
            Stmt::Continue { span } => {
                let (cont, _) = self
                    .loops
                    .last()
                    .ok_or_else(|| CodegenError::new(*span, "`continue` がループの外にあります"))?;
                self.emit(&format!("br label %{cont}"));
                self.terminated = true;
                Ok(())
            }
            Stmt::Expr(e) => {
                let llty = self.expr_llvm_ty(e).unwrap_or_else(|_| "i32".to_string());
                self.gen_expr(e, &llty)?;
                Ok(())
            }
        }
    }

    // ---- ループ ---------------------------------------------------------

    fn gen_while(&mut self, cond: &Expr, body: &Block) -> Result<(), CodegenError> {
        let cond_l = self.fresh_label("while.cond");
        let body_l = self.fresh_label("while.body");
        let end_l = self.fresh_label("while.end");

        self.emit(&format!("br label %{cond_l}"));
        self.emit_label(&cond_l);
        let (c, _) = self.gen_value(cond, &Ty::named("bool"))?;
        self.emit(&format!("br i1 {c}, label %{body_l}, label %{end_l}"));

        self.emit_label(&body_l);
        // continue は条件へ、break は末尾へ。
        self.loops.push((cond_l.clone(), end_l.clone()));
        self.gen_block(body)?;
        self.loops.pop();
        if !self.terminated {
            self.emit(&format!("br label %{cond_l}"));
        }

        self.emit_label(&end_l);
        Ok(())
    }

    fn gen_loop(&mut self, body: &Block) -> Result<(), CodegenError> {
        let body_l = self.fresh_label("loop.body");
        let end_l = self.fresh_label("loop.end");

        self.emit(&format!("br label %{body_l}"));
        self.emit_label(&body_l);
        // continue は本体先頭へ、break は末尾へ。
        self.loops.push((body_l.clone(), end_l.clone()));
        self.gen_block(body)?;
        self.loops.pop();
        if !self.terminated {
            self.emit(&format!("br label %{body_l}"));
        }

        self.emit_label(&end_l);
        Ok(())
    }

    /// `for x in start..end { ... }` をカウンタループへ落とす。
    /// 範囲境界は一度だけ評価し、`x` を 1 ずつ増やしながら `x < end`（包含なら
    /// `x <= end`）が成り立つ間ループする。`continue` は増分（step）へ、`break` は
    /// 末尾へ分岐する。現状は整数範囲のみ（typeck が境界を整数に制限する）。
    fn gen_for(
        &mut self,
        var_span: &Span,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        body: &Block,
    ) -> Result<(), CodegenError> {
        // ループ変数の型: 境界の具体整数型を優先し、両方リテラルなら既定 `i32`。
        let lo_ty = self.raw_ty(start);
        let hi_ty = self.raw_ty(end);
        let var_ty = if matches!(lo_ty, Ty::Named { .. }) {
            lo_ty
        } else if matches!(hi_ty, Ty::Named { .. }) {
            hi_ty
        } else {
            Ty::named("i32")
        }
        .defaulted();
        let llty = llvm_ty(&var_ty, self.structs)
            .map_err(|m| CodegenError::new(start.span, m))?;
        let nk = num_kind(&var_ty);

        let cond_l = self.fresh_label("for.cond");
        let body_l = self.fresh_label("for.body");
        let step_l = self.fresh_label("for.step");
        let end_l = self.fresh_label("for.end");

        // 初期値をループ変数の場所へ格納する。
        let (init, _) = self.gen_value(start, &var_ty)?;
        let id = self.def_spans.get(var_span).copied().ok_or_else(|| {
            CodegenError::new(*var_span, "ループ変数の定義が見つかりません")
        })?;
        let slot = format!("%v.slot{id}");
        self.emit(&format!("{slot} = alloca {llty}"));
        self.emit(&format!("store {llty} {init}, ptr {slot}"));
        self.locals.insert(id, (slot.clone(), llty.clone()));
        // 上限は一度だけ評価する（preheader でループ全体を支配する）。
        let (limit, _) = self.gen_value(end, &var_ty)?;

        self.emit(&format!("br label %{cond_l}"));
        self.emit_label(&cond_l);
        let cur = self.fresh_tmp();
        self.emit(&format!("{cur} = load {llty}, ptr {slot}"));
        let pred = match (nk, inclusive) {
            (NumKind::UInt, false) => "icmp ult",
            (NumKind::UInt, true) => "icmp ule",
            (NumKind::SInt, false) => "icmp slt",
            (NumKind::SInt, true) => "icmp sle",
            // 整数のみのため Float には来ない（防御的に slt/sle）。
            (NumKind::Float, false) => "icmp slt",
            (NumKind::Float, true) => "icmp sle",
        };
        let c = self.fresh_tmp();
        self.emit(&format!("{c} = {pred} {llty} {cur}, {limit}"));
        self.emit(&format!("br i1 {c}, label %{body_l}, label %{end_l}"));

        self.emit_label(&body_l);
        // continue は増分へ、break は末尾へ。
        self.loops.push((step_l.clone(), end_l.clone()));
        self.gen_block(body)?;
        self.loops.pop();
        if !self.terminated {
            self.emit(&format!("br label %{step_l}"));
        }

        self.emit_label(&step_l);
        let cur2 = self.fresh_tmp();
        self.emit(&format!("{cur2} = load {llty}, ptr {slot}"));
        let next = self.fresh_tmp();
        self.emit(&format!("{next} = add {llty} {cur2}, 1"));
        self.emit(&format!("store {llty} {next}, ptr {slot}"));
        self.emit(&format!("br label %{cond_l}"));

        self.emit_label(&end_l);
        Ok(())
    }

    /// 代入先の場所のポインタと型を返す。
    fn place_ptr(&mut self, target: &Expr) -> Result<(String, String), CodegenError> {
        match &target.kind {
            ExprKind::Ident(_) => {
                if let Some(&id) = self.res.uses.get(&target.span)
                    && let Some((ptr, ty)) = self.locals.get(&id)
                {
                    return Ok((ptr.clone(), ty.clone()));
                }
            }
            // フィールドの場所（`a.b`）はその struct のフィールドアドレス。
            ExprKind::Member { object, field, .. } => {
                let (ptr, fllty, _) = self.field_ptr(object, field, target.span)?;
                return Ok((ptr, fllty));
            }
            _ => {}
        }
        Err(CodegenError::new(
            target.span,
            "この代入先はコード生成に未対応です",
        ))
    }

    /// メンバアクセス `object.field` のフィールドへのポインタを求める。
    /// 戻り値は (フィールドポインタ, フィールドの LLVM 型, フィールドの内部型)。
    fn field_ptr(
        &mut self,
        object: &Expr,
        field: &str,
        span: Span,
    ) -> Result<(String, String, Ty), CodegenError> {
        let (base, sname) = self.struct_base_ptr(object, span)?;
        let (canon, fields) = self
            .structs
            .struct_def(&sname)
            .ok_or_else(|| CodegenError::new(span, format!("型 `{sname}` は構造体ではありません")))?;
        let idx = fields
            .iter()
            .position(|(n, _)| n == field)
            .ok_or_else(|| {
                CodegenError::new(span, format!("型 `{canon}` にフィールド `{field}` はありません"))
            })?;
        let fty = fields[idx].1.clone();
        let fllty = llvm_ty(&fty, self.structs).map_err(|m| CodegenError::new(span, m))?;
        let p = self.fresh_tmp();
        self.emit(&format!(
            "{p} = getelementptr inbounds %{canon}, ptr {base}, i32 0, i32 {idx}"
        ));
        Ok((p, fllty, fty))
    }

    /// メンバアクセスの基底となる struct のポインタと（正規化前の）struct 名を返す。
    /// 参照越しのアクセスは参照値（ポインタ）を辿る。場所でない値（関数の戻り値など）は
    /// 一時 alloca に退避してアドレスを得る。
    fn struct_base_ptr(
        &mut self,
        object: &Expr,
        span: Span,
    ) -> Result<(String, String), CodegenError> {
        let oty = self.raw_ty(object).defaulted();
        if let Ty::Ref { .. } = oty {
            // object は値としてポインタを返す。多段参照は値の文脈で 1 段ずつ load する。
            let mut ptr = self.gen_expr(object, "ptr")?;
            let mut cur = oty;
            while let Ty::Ref { inner, .. } = cur {
                let inner = *inner;
                // 指す先がさらに参照なら、そのポインタを load して辿る。
                if let Ty::Ref { .. } = inner {
                    let r = self.fresh_tmp();
                    self.emit(&format!("{r} = load ptr, ptr {ptr}"));
                    ptr = r;
                    cur = inner;
                } else {
                    cur = inner;
                    break;
                }
            }
            let name = struct_name_of(&cur).ok_or_else(|| {
                CodegenError::new(span, "メンバアクセスの対象が構造体ではありません")
            })?;
            return Ok((ptr, name));
        }
        // 場所ならそのアドレス、そうでなければ一時 alloca に退避する。
        let name = struct_name_of(&oty)
            .ok_or_else(|| CodegenError::new(span, "メンバアクセスの対象が構造体ではありません"))?;
        match self.place_ptr(object) {
            Ok((p, _)) => Ok((p, name)),
            Err(_) => {
                let llty = llvm_ty(&oty, self.structs).map_err(|m| CodegenError::new(span, m))?;
                let val = self.gen_expr(object, &llty)?;
                let slot = self.fresh_tmp();
                self.emit(&format!("{slot} = alloca {llty}"));
                self.emit(&format!("store {llty} {val}, ptr {slot}"));
                Ok((slot, name))
            }
        }
    }

    /// struct の `==` / `!=` を構造的に生成する（ADR-0009）。各フィールドを再帰的に
    /// 比較し AND で合成する。`negate` のとき結果を反転（`!=`）。
    fn gen_struct_eq(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        sname: &str,
        negate: bool,
        span: Span,
    ) -> Result<String, CodegenError> {
        let (lbase, _) = self.struct_base_ptr(lhs, span)?;
        let (rbase, _) = self.struct_base_ptr(rhs, span)?;
        let ty = Ty::named(sname);
        let eq = self.gen_eq_at(&lbase, &rbase, &ty, span)?;
        if negate {
            let r = self.fresh_tmp();
            self.emit(&format!("{r} = xor i1 {eq}, true"));
            Ok(r)
        } else {
            Ok(eq)
        }
    }

    /// 2 つのポインタが指す型 `ty` の値の等価性（i1）を生成する。struct は再帰的に、
    /// プリミティブ・参照は load して比較する。
    fn gen_eq_at(
        &mut self,
        lptr: &str,
        rptr: &str,
        ty: &Ty,
        span: Span,
    ) -> Result<String, CodegenError> {
        if let Some(sname) = struct_name_of(ty)
            && let Some((canon, fields)) =
                self.structs.struct_def(&sname).map(|(c, f)| (c, f.clone()))
        {
            let mut acc: Option<String> = None;
            for (idx, (_, fty)) in fields.iter().enumerate() {
                let lp = self.fresh_tmp();
                self.emit(&format!(
                    "{lp} = getelementptr inbounds %{canon}, ptr {lptr}, i32 0, i32 {idx}"
                ));
                let rp = self.fresh_tmp();
                self.emit(&format!(
                    "{rp} = getelementptr inbounds %{canon}, ptr {rptr}, i32 0, i32 {idx}"
                ));
                let cmp = self.gen_eq_at(&lp, &rp, fty, span)?;
                acc = Some(match acc {
                    None => cmp,
                    Some(a) => {
                        let r = self.fresh_tmp();
                        self.emit(&format!("{r} = and i1 {a}, {cmp}"));
                        r
                    }
                });
            }
            // フィールドの無い struct は常に等しい。
            return Ok(acc.unwrap_or_else(|| "true".to_string()));
        }
        // プリミティブ・参照: load して比較する。
        let llty = llvm_ty(ty, self.structs).map_err(|m| CodegenError::new(span, m))?;
        let lv = self.fresh_tmp();
        self.emit(&format!("{lv} = load {llty}, ptr {lptr}"));
        let rv = self.fresh_tmp();
        self.emit(&format!("{rv} = load {llty}, ptr {rptr}"));
        let r = self.fresh_tmp();
        if ty.is_float() {
            self.emit(&format!("{r} = fcmp oeq {llty} {lv}, {rv}"));
        } else {
            self.emit(&format!("{r} = icmp eq {llty} {lv}, {rv}"));
        }
        Ok(r)
    }

    /// 構造体リテラル `Name { field: value, ... }` を生成し、構造体値を返す。
    fn gen_struct_lit(
        &mut self,
        name: &str,
        fields: &[FieldInit],
        span: Span,
    ) -> Result<String, CodegenError> {
        let (canon, def_fields) = self
            .structs
            .struct_def(name)
            .map(|(c, f)| (c, f.clone()))
            .ok_or_else(|| {
                CodegenError::new(
                    span,
                    format!("`{name}` の構造体定義が見つかりません（ジェネリック struct は未対応）"),
                )
            })?;
        let slot = self.fresh_tmp();
        self.emit(&format!("{slot} = alloca %{canon}"));
        // 宣言順にフィールドを書き込む。
        for (idx, (fname, fty)) in def_fields.iter().enumerate() {
            let init = fields.iter().find(|fi| &fi.name == fname).ok_or_else(|| {
                CodegenError::new(span, format!("フィールド `{fname}` が初期化されていません"))
            })?;
            let (v, fllty) = self.gen_value(&init.value, fty)?;
            let p = self.fresh_tmp();
            self.emit(&format!(
                "{p} = getelementptr inbounds %{canon}, ptr {slot}, i32 0, i32 {idx}"
            ));
            self.emit(&format!("store {fllty} {v}, ptr {p}"));
        }
        // 構造体値として読み出す（first-class 値）。
        let r = self.fresh_tmp();
        self.emit(&format!("{r} = load %{canon}, ptr {slot}"));
        Ok(r)
    }

    // ---- 式 -------------------------------------------------------------

    /// enum バリアントを構築し、`{ i8, i64 }` 値を返す。
    /// `enum_name`・`tag`・ペイロード expr を受け取る。
    fn gen_enum_construction(
        &mut self,
        enum_name: &str,
        tag: usize,
        payload_expr: Option<&Expr>,
        span: Span,
    ) -> Result<String, CodegenError> {
        let slot = self.fresh_tmp();
        self.emit(&format!("{slot} = alloca %{enum_name}"));
        // タグを書き込む。
        let tag_ptr = self.fresh_tmp();
        self.emit(&format!(
            "{tag_ptr} = getelementptr inbounds %{enum_name}, ptr {slot}, i32 0, i32 0"
        ));
        self.emit(&format!("store i8 {tag}, ptr {tag_ptr}"));
        // ペイロードを書き込む（i64 にキャスト）。
        if let Some(pe) = payload_expr {
            let payload_ty = self.iris_ty(pe);
            let (v, vllty) = self.gen_value(pe, &payload_ty)?;
            let payload_i64 = self.cast_to_i64(&v, &vllty, span)?;
            let payload_ptr = self.fresh_tmp();
            self.emit(&format!(
                "{payload_ptr} = getelementptr inbounds %{enum_name}, ptr {slot}, i32 0, i32 1"
            ));
            self.emit(&format!("store i64 {payload_i64}, ptr {payload_ptr}"));
        } else {
            // ペイロードなしは 0 を格納。
            let payload_ptr = self.fresh_tmp();
            self.emit(&format!(
                "{payload_ptr} = getelementptr inbounds %{enum_name}, ptr {slot}, i32 0, i32 1"
            ));
            self.emit(&format!("store i64 0, ptr {payload_ptr}"));
        }
        let r = self.fresh_tmp();
        self.emit(&format!("{r} = load %{enum_name}, ptr {slot}"));
        Ok(r)
    }

    /// 値を i64 にキャストする（enum ペイロード格納用）。
    fn cast_to_i64(&mut self, v: &str, llty: &str, span: Span) -> Result<String, CodegenError> {
        if llty == "i64" {
            return Ok(v.to_string());
        }
        let r = self.fresh_tmp();
        let instr = match llty {
            "i1" | "i8" | "i16" | "i32" => format!("{r} = zext {llty} {v} to i64"),
            "f32" => {
                let tmp = self.fresh_tmp();
                self.emit(&format!("{tmp} = fpext float {v} to double"));
                format!("{r} = bitcast double {tmp} to i64")
            }
            "f64" | "double" => format!("{r} = bitcast double {v} to i64"),
            "ptr" => format!("{r} = ptrtoint ptr {v} to i64"),
            other => {
                return Err(CodegenError::new(
                    span,
                    format!("enum ペイロードの型 `{other}` は未対応です（i64 に変換できません）"),
                ));
            }
        };
        self.emit(&instr);
        Ok(r)
    }

    /// i64 から元の型にキャストして戻す（enum ペイロード読み出し用）。
    fn cast_from_i64(&mut self, v: &str, llty: &str, span: Span) -> Result<String, CodegenError> {
        if llty == "i64" {
            return Ok(v.to_string());
        }
        let r = self.fresh_tmp();
        let instr = match llty {
            "i1" | "i8" | "i16" | "i32" => format!("{r} = trunc i64 {v} to {llty}"),
            "f32" => {
                let tmp = self.fresh_tmp();
                self.emit(&format!("{tmp} = bitcast i64 {v} to double"));
                format!("{r} = fptrunc double {tmp} to float")
            }
            "f64" | "double" => format!("{r} = bitcast i64 {v} to double"),
            "ptr" => format!("{r} = inttoptr i64 {v} to ptr"),
            other => {
                return Err(CodegenError::new(
                    span,
                    format!("enum ペイロードの型 `{other}` は未対応です（i64 から変換できません）"),
                ));
            }
        };
        self.emit(&instr);
        Ok(r)
    }

    /// 式を評価し、結果の値（レジスタまたは定数）を返す。`hint` はリテラルの型。
    fn gen_expr(&mut self, expr: &Expr, hint: &str) -> Result<String, CodegenError> {
        // enum バリアント構築（typeck が記録。Ident/Call のどちらでも来る）。
        if let Some((enum_name, tag, has_payload)) = self.variant_constructions.get(&expr.span).cloned() {
            let payload_expr = if has_payload {
                // Call の場合、最初の引数がペイロード。
                match &expr.kind {
                    ExprKind::Call { args, .. } => args.first().map(|e| e as &Expr),
                    _ => None,
                }
            } else {
                None
            };
            return self.gen_enum_construction(&enum_name, tag, payload_expr, expr.span);
        }
        match &expr.kind {
            ExprKind::Int(v) => Ok(v.to_string()),
            ExprKind::Float(v) => Ok(float_const(*v, hint)),
            ExprKind::Bool(b) => Ok(if *b { "1" } else { "0" }.to_string()),
            // 文字列リテラルはグローバル定数（NUL 終端）にし、その記号を `ptr` 値として返す。
            ExprKind::Str(s) => Ok(self.strings.borrow_mut().intern(s)),
            ExprKind::Ident(_) => {
                let (ptr, ty) = self.lookup(expr)?;
                let r = self.fresh_tmp();
                self.emit(&format!("{r} = load {ty}, ptr {ptr}"));
                Ok(r)
            }
            ExprKind::Unary { op, expr: inner } => self.gen_unary(*op, inner, expr.span),
            ExprKind::Binary { op, lhs, rhs } => self.gen_binary(*op, lhs, rhs, expr.span),
            ExprKind::Call { callee, args } => self.gen_call(callee, args, expr.span),
            // メンバアクセス `a.b`: フィールドのアドレスを求めて load する。
            ExprKind::Member { object, field, .. } => {
                let (ptr, fllty, _) = self.field_ptr(object, field, expr.span)?;
                let r = self.fresh_tmp();
                self.emit(&format!("{r} = load {fllty}, ptr {ptr}"));
                Ok(r)
            }
            // 構造体リテラル `Name { ... }`。
            ExprKind::StructLit { name, fields, .. } => {
                self.gen_struct_lit(name, fields, expr.span)
            }
            ExprKind::Ternary {
                cond,
                then,
                otherwise,
            } => {
                let resty = self.expr_llvm_ty(expr).unwrap_or_else(|_| hint.to_string());
                self.gen_select(cond, Branch::Expr(then), Branch::Expr(otherwise), &resty)
            }
            // if は文として使われる（値を持たない）。
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.gen_if(cond, then, otherwise.as_deref())?;
                Ok(String::new())
            }
            ExprKind::Try(_) => {
                Err(CodegenError::new(expr.span, "`!` 演算子のコード生成は未対応です"))
            }
            ExprKind::EnumLit { span, .. } => {
                Err(CodegenError::new(*span, "この enum リテラルのコード生成は未対応です"))
            }
            ExprKind::Match { scrutinee, arms } => {
                self.gen_match(scrutinee, arms, expr.span)
            }
        }
    }

    /// match 式を生成する。結果型が void でなければ alloca+store+load で値を返す。
    fn gen_match(&mut self, scrutinee: &Expr, arms: &[MatchArm], span: Span) -> Result<String, CodegenError> {
        // 結果型の LLVM 型を求める（最初の非 void アームから判断）。
        let result_llty = self.types.get(&span)
            .cloned()
            .unwrap_or(Ty::unit())
            .defaulted();
        let is_void = result_llty == Ty::unit();
        let result_llty_str = if is_void {
            "void".to_string()
        } else {
            llvm_ty(&result_llty, self.structs)
                .map_err(|m| CodegenError::new(span, m))?
        };

        // 結果スロット（void でなければ確保）。
        let result_slot = if !is_void {
            let s = self.fresh_tmp();
            self.emit(&format!("{s} = alloca {result_llty_str}"));
            Some(s)
        } else {
            None
        };

        let merge_l = self.fresh_label("match.end");

        // scrutinee の型から enum かどうか判定。
        let scrut_ty = self.iris_ty(scrutinee);
        let enum_name: Option<String> = match &scrut_ty {
            Ty::Named { name, .. } if self.enums.variants(name).is_some() => Some(name.clone()),
            _ => None,
        };

        // scrutinee を評価する。enum の場合はポインタ経由でタグを読みたいので alloca。
        let (scrut_val, scrut_llty) = if let Some(ref ename) = enum_name {
            // enum: alloca に格納してポインタ経由でタグを読む。
            let ev = self.gen_value(scrutinee, &scrut_ty)?;
            let slot = self.fresh_tmp();
            self.emit(&format!("{slot} = alloca %{ename}"));
            self.emit(&format!("store %{ename} {}, ptr {slot}", ev.0));
            (slot, format!("%{ename}"))
        } else {
            let v = self.gen_value(scrutinee, &scrut_ty)?;
            (v.0, v.1)
        };

        // タグを読み出す（enum の場合）。
        let tag_val = if let Some(ref ename) = enum_name {
            let tag_ptr = self.fresh_tmp();
            self.emit(&format!(
                "{tag_ptr} = getelementptr inbounds %{ename}, ptr {scrut_val}, i32 0, i32 0"
            ));
            let tv = self.fresh_tmp();
            self.emit(&format!("{tv} = load i8, ptr {tag_ptr}"));
            Some(tv)
        } else {
            None
        };

        // アームを if-else チェーンで出力。
        for (i, arm) in arms.iter().enumerate() {
            let is_last = i == arms.len() - 1;
            let arm_l = self.fresh_label("match.arm");
            let skip_l = if is_last {
                merge_l.clone()
            } else {
                self.fresh_label("match.check")
            };

            // パターン条件チェック。
            let cond = match &arm.pattern {
                Pattern::Wildcard { .. } => {
                    // ワイルドカードは常に一致。
                    self.emit(&format!("br label %{arm_l}"));
                    None
                }
                Pattern::Lit { value, .. } => {
                    match value {
                        // 文字列は NUL 終端 ptr 同士を strcmp で比較する（一致＝0）。
                        LitPat::Str(s) => {
                            let gref = self.strings.borrow_mut().intern(s);
                            let r = self.fresh_tmp();
                            self.emit(&format!(
                                "{r} = call i32 @strcmp(ptr {scrut_val}, ptr {gref})"
                            ));
                            let cmp = self.fresh_tmp();
                            self.emit(&format!("{cmp} = icmp eq i32 {r}, 0"));
                            Some(cmp)
                        }
                        _ => {
                            let pat_v = match value {
                                LitPat::Int(n) => n.to_string(),
                                LitPat::Float(f) => float_const(*f, &scrut_llty),
                                LitPat::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                                LitPat::Str(_) => unreachable!(),
                            };
                            let cmp = self.fresh_tmp();
                            if scrut_ty.is_float() {
                                self.emit(&format!(
                                    "{cmp} = fcmp oeq {scrut_llty} {scrut_val}, {pat_v}"
                                ));
                            } else {
                                self.emit(&format!(
                                    "{cmp} = icmp eq {scrut_llty} {scrut_val}, {pat_v}"
                                ));
                            }
                            Some(cmp)
                        }
                    }
                }
                Pattern::Range { lo, hi, inclusive, .. } => {
                    // 数値の範囲 `lo..hi` / `lo..=hi`: lo <= x && (x < hi | x <= hi)。
                    let nk = num_kind(&scrut_ty);
                    let const_of = |b: &LitPat| match b {
                        LitPat::Int(n) => n.to_string(),
                        LitPat::Float(f) => float_const(*f, &scrut_llty),
                        LitPat::Bool(x) => if *x { "1" } else { "0" }.to_string(),
                        LitPat::Str(_) => "0".to_string(),
                    };
                    let lo_v = const_of(lo);
                    let hi_v = const_of(hi);
                    let ge_pred = match nk {
                        NumKind::Float => "fcmp oge",
                        NumKind::UInt => "icmp uge",
                        NumKind::SInt => "icmp sge",
                    };
                    let hi_pred = match (nk, *inclusive) {
                        (NumKind::Float, false) => "fcmp olt",
                        (NumKind::Float, true) => "fcmp ole",
                        (NumKind::UInt, false) => "icmp ult",
                        (NumKind::UInt, true) => "icmp ule",
                        (NumKind::SInt, false) => "icmp slt",
                        (NumKind::SInt, true) => "icmp sle",
                    };
                    let ge = self.fresh_tmp();
                    self.emit(&format!("{ge} = {ge_pred} {scrut_llty} {scrut_val}, {lo_v}"));
                    let lt = self.fresh_tmp();
                    self.emit(&format!("{lt} = {hi_pred} {scrut_llty} {scrut_val}, {hi_v}"));
                    let and = self.fresh_tmp();
                    self.emit(&format!("{and} = and i1 {ge}, {lt}"));
                    Some(and)
                }
                Pattern::Variant { name, binding, .. } => {
                    // enum タグとの比較。
                    let ename = enum_name.as_deref().ok_or_else(|| {
                        CodegenError::new(span, "バリアントパターンを非 enum 型に使っています")
                    })?;
                    let tag = self.enums.tag_of(ename, name).ok_or_else(|| {
                        CodegenError::new(span, format!("バリアント `{name}` が見つかりません"))
                    })?;
                    let cmp = self.fresh_tmp();
                    let tv = tag_val.as_deref().unwrap();
                    self.emit(&format!("{cmp} = icmp eq i8 {tv}, {tag}"));
                    Some(cmp)
                }
            };

            if let Some(c) = cond {
                self.emit(&format!("br i1 {c}, label %{arm_l}, label %{skip_l}"));
            }

            self.emit_label(&arm_l);
            self.terminated = false;

            // バリアントパターンのペイロード束縛を設定する。
            if let Pattern::Variant { name, binding: Some((bname, bspan)), .. } = &arm.pattern {
                let ename = enum_name.as_deref().unwrap();
                if let Some(vs) = self.enums.variants(ename) {
                    if let Some((_, Some(payload_ty))) = vs.iter().find(|(n, _)| n == name) {
                        let payload_ty = payload_ty.clone();
                        let pllty = llvm_ty(&payload_ty, self.structs)
                            .map_err(|m| CodegenError::new(span, m))?;
                        // ペイロードを i64 フィールドから読み出してキャスト。
                        let payload_ptr = self.fresh_tmp();
                        self.emit(&format!(
                            "{payload_ptr} = getelementptr inbounds %{ename}, ptr {scrut_val}, i32 0, i32 1"
                        ));
                        let raw = self.fresh_tmp();
                        self.emit(&format!("{raw} = load i64, ptr {payload_ptr}"));
                        let typed = self.cast_from_i64(&raw, &pllty, span)?;
                        // 束縛変数を alloca に格納する。
                        if let Some(&id) = self.def_spans.get(bspan) {
                            let slot = format!("%{bname}.slot{id}");
                            self.emit(&format!("{slot} = alloca {pllty}"));
                            self.emit(&format!("store {pllty} {typed}, ptr {slot}"));
                            self.locals.insert(id, (slot, pllty));
                        }
                    }
                }
            }

            // ガード: 束縛変数を設定した後に評価し、不成立なら次のチェック（skip_l）へ
            // フォールスルーする。
            if let Some(g) = &arm.guard {
                let (gv, _) = self.gen_value(g, &Ty::named("bool"))?;
                let guarded_l = self.fresh_label("match.guarded");
                self.emit(&format!("br i1 {gv}, label %{guarded_l}, label %{skip_l}"));
                self.emit_label(&guarded_l);
                self.terminated = false;
            }

            // アーム本体を生成して結果を格納。
            if !is_void {
                let rs = result_slot.as_deref().unwrap();
                let (v, vty) = self.gen_value(&arm.body, &result_llty)?;
                self.emit(&format!("store {vty} {v}, ptr {rs}"));
            } else {
                let llty = self.expr_llvm_ty(&arm.body).unwrap_or_else(|_| "i32".to_string());
                self.gen_expr(&arm.body, &llty)?;
            }

            if !self.terminated {
                self.emit(&format!("br label %{merge_l}"));
            }

            // 次のチェックラベルに移行（最後のアームは merge_l へ）。
            if !is_last {
                self.emit_label(&skip_l);
                self.terminated = false;
            }
        }

        self.emit_label(&merge_l);
        self.terminated = false;

        // 結果値を読み出す。
        if let Some(rs) = result_slot {
            let r = self.fresh_tmp();
            self.emit(&format!("{r} = load {result_llty_str}, ptr {rs}"));
            Ok(r)
        } else {
            Ok(String::new())
        }
    }

    fn gen_unary(&mut self, op: UnaryOp, inner: &Expr, span: Span) -> Result<String, CodegenError> {
        match op {
            UnaryOp::Neg => {
                // 被演算子が参照なら暗黙にデリファレンスして数値を得る。
                let ity = self.iris_ty(inner).peel_refs().clone();
                let (v, ty) = self.gen_value(inner, &ity)?;
                let r = self.fresh_tmp();
                if num_kind(&ity) == NumKind::Float {
                    self.emit(&format!("{r} = fneg {ty} {v}"));
                } else {
                    self.emit(&format!("{r} = sub {ty} 0, {v}"));
                }
                Ok(r)
            }
            // `&x` / `&mut x` は場所のアドレス（ポインタ）を値として返す。
            UnaryOp::Ref | UnaryOp::RefMut => {
                let (ptr, _) = self.place_ptr(inner).map_err(|_| {
                    CodegenError::new(span, "この場所の参照はコード生成に未対応です")
                })?;
                Ok(ptr)
            }
        }
    }

    fn gen_binary(
        &mut self,
        op: BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<String, CodegenError> {
        // 論理演算は短絡（三項として実装）。
        match op {
            BinaryOp::And => {
                return self.gen_select(lhs, Branch::Expr(rhs), Branch::ConstBool(false), "i1");
            }
            BinaryOp::Or => {
                return self.gen_select(lhs, Branch::ConstBool(true), Branch::Expr(rhs), "i1");
            }
            _ => {}
        }

        // struct の `==` / `!=` は構造的フィールド比較（ADR-0009）。
        if matches!(op, BinaryOp::Eq | BinaryOp::NotEq) {
            let lty = self.raw_ty(lhs).peel_refs().clone();
            if let Some(sname) = struct_name_of(&lty)
                && self.structs.struct_def(&sname).is_some()
            {
                return self.gen_struct_eq(lhs, rhs, &sname, op == BinaryOp::NotEq, span);
            }
        }

        let (chosen, opnd_ty, kind) = self.operand_ty(lhs, rhs)?;
        // 参照の被演算子は gen_value が暗黙にデリファレンスする。
        let (l, _) = self.gen_value(lhs, &chosen)?;
        let (r, _) = self.gen_value(rhs, &chosen)?;
        let res = self.fresh_tmp();

        // 算術・比較の命令は数値クラス（符号付き/なし整数・浮動小数）で変わる。
        let inst = match (op, kind) {
            (BinaryOp::Add, NumKind::Float) => format!("fadd {opnd_ty} {l}, {r}"),
            (BinaryOp::Add, _) => format!("add {opnd_ty} {l}, {r}"),
            (BinaryOp::Sub, NumKind::Float) => format!("fsub {opnd_ty} {l}, {r}"),
            (BinaryOp::Sub, _) => format!("sub {opnd_ty} {l}, {r}"),
            (BinaryOp::Mul, NumKind::Float) => format!("fmul {opnd_ty} {l}, {r}"),
            (BinaryOp::Mul, _) => format!("mul {opnd_ty} {l}, {r}"),
            (BinaryOp::Div, NumKind::Float) => format!("fdiv {opnd_ty} {l}, {r}"),
            (BinaryOp::Div, NumKind::UInt) => format!("udiv {opnd_ty} {l}, {r}"),
            (BinaryOp::Div, NumKind::SInt) => format!("sdiv {opnd_ty} {l}, {r}"),
            (BinaryOp::Rem, NumKind::Float) => format!("frem {opnd_ty} {l}, {r}"),
            (BinaryOp::Rem, NumKind::UInt) => format!("urem {opnd_ty} {l}, {r}"),
            (BinaryOp::Rem, NumKind::SInt) => format!("srem {opnd_ty} {l}, {r}"),
            (BinaryOp::Eq, NumKind::Float) => format!("fcmp oeq {opnd_ty} {l}, {r}"),
            (BinaryOp::Eq, _) => format!("icmp eq {opnd_ty} {l}, {r}"),
            (BinaryOp::NotEq, NumKind::Float) => format!("fcmp one {opnd_ty} {l}, {r}"),
            (BinaryOp::NotEq, _) => format!("icmp ne {opnd_ty} {l}, {r}"),
            (BinaryOp::Lt, NumKind::Float) => format!("fcmp olt {opnd_ty} {l}, {r}"),
            (BinaryOp::Lt, NumKind::UInt) => format!("icmp ult {opnd_ty} {l}, {r}"),
            (BinaryOp::Lt, NumKind::SInt) => format!("icmp slt {opnd_ty} {l}, {r}"),
            (BinaryOp::LtEq, NumKind::Float) => format!("fcmp ole {opnd_ty} {l}, {r}"),
            (BinaryOp::LtEq, NumKind::UInt) => format!("icmp ule {opnd_ty} {l}, {r}"),
            (BinaryOp::LtEq, NumKind::SInt) => format!("icmp sle {opnd_ty} {l}, {r}"),
            (BinaryOp::Gt, NumKind::Float) => format!("fcmp ogt {opnd_ty} {l}, {r}"),
            (BinaryOp::Gt, NumKind::UInt) => format!("icmp ugt {opnd_ty} {l}, {r}"),
            (BinaryOp::Gt, NumKind::SInt) => format!("icmp sgt {opnd_ty} {l}, {r}"),
            (BinaryOp::GtEq, NumKind::Float) => format!("fcmp oge {opnd_ty} {l}, {r}"),
            (BinaryOp::GtEq, NumKind::UInt) => format!("icmp uge {opnd_ty} {l}, {r}"),
            (BinaryOp::GtEq, NumKind::SInt) => format!("icmp sge {opnd_ty} {l}, {r}"),
            (BinaryOp::And | BinaryOp::Or, _) => {
                return Err(CodegenError::new(span, "論理演算の内部エラー"));
            }
        };
        self.emit(&format!("{res} = {inst}"));
        Ok(res)
    }

    fn gen_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        span: Span,
    ) -> Result<String, CodegenError> {
        // メソッド呼び出し `object.method(args)`。
        if let ExprKind::Member {
            object,
            field,
            qualifier,
        } = &callee.kind
        {
            return self.gen_method_call(
                object,
                field,
                qualifier.as_ref(),
                callee.span,
                args,
                span,
            );
        }
        let ExprKind::Ident(name) = &callee.kind else {
            return Err(CodegenError::new(span, "この呼び出しはコード生成に未対応です"));
        };
        // 関数のシグネチャから引数型・戻り値型を引く。
        let func = self
            .find_function(name)
            .ok_or_else(|| CodegenError::new(span, format!("`{name}` の定義が見つかりません")))?;

        // ジェネリック関数なら、単相化記号を引き、callee 側の型パラメータ束縛で型を解決する。
        let (symbol, callee_subst) = if func.generics.is_empty() {
            (name.clone(), HashMap::new())
        } else {
            let (gname, abstract_args) = self.mono.get(&callee.span).ok_or_else(|| {
                CodegenError::new(span, format!("`{name}` の単相化情報がありません"))
            })?;
            // 入れ子の場合に備え、外側の型パラメータ束縛を適用して具体化する。
            let concrete: Vec<Ty> = abstract_args.iter().map(|a| subst_ty(a, self.type_subst)).collect();
            let mut cmap = HashMap::new();
            for (g, a) in func.generics.iter().zip(&concrete) {
                cmap.insert(g.name.clone(), a.clone());
            }
            (mono_symbol(gname, &concrete), cmap)
        };

        let lower_callee = |t: &Type| subst_ty(&Ty::from_ast(t), &callee_subst);
        let ret_ty = match &func.ret {
            Some(t) => llvm_ty(&lower_callee(t), self.structs).map_err(|m| CodegenError::new(t.span(), m))?,
            None => "void".to_string(),
        };
        let mut arg_strs = Vec::new();
        for (i, a) in args.iter().enumerate() {
            // 仮引数の型を期待型として渡す。引数が参照で仮引数が値型なら
            // gen_value が暗黙にデリファレンスする。
            let want = func
                .params
                .get(i)
                .map(|p| lower_callee(&p.ty))
                .unwrap_or(Ty::Infer);
            let (v, pty) = self.gen_value(a, &want)?;
            arg_strs.push(format!("{pty} {v}"));
        }
        let call = format!("call {ret_ty} @{symbol}({})", arg_strs.join(", "));
        if ret_ty == "void" {
            self.emit(&call);
            Ok(String::new())
        } else {
            let r = self.fresh_tmp();
            self.emit(&format!("{r} = {call}"));
            Ok(r)
        }
    }

    /// メソッド呼び出し `object.method(args)` を直接呼び出しに落とす。
    /// self は受け方に応じて先頭引数として渡す（値＝struct値、`&self`/`&mut self`＝ptr）。
    fn gen_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        qualifier: Option<&crate::ast::TraitRef>,
        member_span: Span,
        args: &[Expr],
        span: Span,
    ) -> Result<String, CodegenError> {
        let recv_ty = self.raw_ty(object).defaulted();
        let ty_name = struct_name_of(&recv_ty)
            .ok_or_else(|| CodegenError::new(span, "メソッド呼び出しの受け手が型を持ちません"))?;

        // 提供元ラベル: `#修飾子` があればそれ、無ければ typeck が記録した解決結果、
        // どちらも無ければ固有（型名）とみなす。
        let label = qualifier
            .map(|q| q.name.clone())
            .or_else(|| self.method_provider.get(&member_span).cloned())
            .unwrap_or_else(|| ty_name.clone());

        let func = *self
            .method_defs
            .get(&(ty_name.clone(), method.to_string(), label.clone()))
            .ok_or_else(|| {
                CodegenError::new(span, format!("メソッド `{ty_name}.{method}` が見つかりません"))
            })?;
        let self_kind = func
            .self_kind
            .ok_or_else(|| CodegenError::new(span, format!("`{method}` は self を取りません")))?;

        let colliding = self
            .method_collisions
            .contains(&(ty_name.clone(), method.to_string()));
        let symbol = method_symbol(&ty_name, method, &label, colliding);

        let ret_ty = match &func.ret {
            Some(t) => llvm_ty(&self.lower(t), self.structs)
                .map_err(|m| CodegenError::new(t.span(), m))?,
            None => "void".to_string(),
        };

        // 第一引数 self。
        let mut arg_strs = Vec::new();
        match self_kind {
            SelfKind::Value => {
                let (sv, sty) = self.gen_value(object, &Ty::named(&ty_name))?;
                arg_strs.push(format!("{sty} {sv}"));
            }
            SelfKind::Ref | SelfKind::RefMut => {
                let ptr = self.self_pointer(object, &recv_ty)?;
                arg_strs.push(format!("ptr {ptr}"));
            }
        }

        // 残りの引数。`func.params` の先頭は合成 self なので 1 つずらして対応づける。
        for (i, a) in args.iter().enumerate() {
            let want = func
                .params
                .get(i + 1)
                .map(|p| self.lower(&p.ty))
                .unwrap_or(Ty::Infer);
            let (v, pty) = self.gen_value(a, &want)?;
            arg_strs.push(format!("{pty} {v}"));
        }

        let call = format!("call {ret_ty} @{symbol}({})", arg_strs.join(", "));
        if ret_ty == "void" {
            self.emit(&call);
            Ok(String::new())
        } else {
            let r = self.fresh_tmp();
            self.emit(&format!("{r} = {call}"));
            Ok(r)
        }
    }

    /// `&self`/`&mut self` メソッドへ渡す self ポインタを求める。受け手が参照なら
    /// その参照値（ptr）、場所ならアドレス、それ以外は一時 alloca へ退避する。
    fn self_pointer(&mut self, object: &Expr, recv_ty: &Ty) -> Result<String, CodegenError> {
        if matches!(recv_ty, Ty::Ref { .. }) {
            return self.gen_expr(object, "ptr");
        }
        match self.place_ptr(object) {
            Ok((p, _)) => Ok(p),
            Err(_) => {
                let llty = llvm_ty(recv_ty, self.structs)
                    .map_err(|m| CodegenError::new(object.span, m))?;
                let val = self.gen_expr(object, &llty)?;
                let slot = self.fresh_tmp();
                self.emit(&format!("{slot} = alloca {llty}"));
                self.emit(&format!("store {llty} {val}, ptr {slot}"));
                Ok(slot)
            }
        }
    }

    /// 三項（および短絡論理）を基本ブロックで生成する。
    fn gen_select(
        &mut self,
        cond: &Expr,
        then: Branch,
        els: Branch,
        resty: &str,
    ) -> Result<String, CodegenError> {
        let slot = self.fresh_tmp();
        self.emit(&format!("{slot} = alloca {resty}"));
        let (c, _) = self.gen_value(cond, &Ty::named("bool"))?;
        let then_l = self.fresh_label("tern.then");
        let else_l = self.fresh_label("tern.else");
        let merge_l = self.fresh_label("tern.end");
        self.emit(&format!("br i1 {c}, label %{then_l}, label %{else_l}"));

        self.emit_label(&then_l);
        let tv = self.gen_branch(&then, resty)?;
        self.emit(&format!("store {resty} {tv}, ptr {slot}"));
        self.emit(&format!("br label %{merge_l}"));

        self.emit_label(&else_l);
        let ev = self.gen_branch(&els, resty)?;
        self.emit(&format!("store {resty} {ev}, ptr {slot}"));
        self.emit(&format!("br label %{merge_l}"));

        self.emit_label(&merge_l);
        let r = self.fresh_tmp();
        self.emit(&format!("{r} = load {resty}, ptr {slot}"));
        Ok(r)
    }

    fn gen_branch(&mut self, b: &Branch, resty: &str) -> Result<String, CodegenError> {
        match b {
            Branch::Expr(e) => self.gen_expr(e, resty),
            Branch::ConstBool(v) => Ok(if *v { "1" } else { "0" }.to_string()),
        }
    }

    // ---- if 文 ----------------------------------------------------------

    fn gen_if(
        &mut self,
        cond: &Expr,
        then: &Block,
        otherwise: Option<&Else>,
    ) -> Result<(), CodegenError> {
        let (c, _) = self.gen_value(cond, &Ty::named("bool"))?;
        let then_l = self.fresh_label("if.then");
        let merge_l = self.fresh_label("if.end");
        let else_l = if otherwise.is_some() {
            self.fresh_label("if.else")
        } else {
            merge_l.clone()
        };
        self.emit(&format!("br i1 {c}, label %{then_l}, label %{else_l}"));

        self.emit_label(&then_l);
        self.terminated = false;
        self.gen_block(then)?;
        let then_terminated = self.terminated;
        if !then_terminated {
            self.emit(&format!("br label %{merge_l}"));
        }

        let else_terminated = if let Some(els) = otherwise {
            self.emit_label(&else_l);
            self.terminated = false;
            match els {
                Else::Block(b) => self.gen_block(b)?,
                Else::If(e) => {
                    if let ExprKind::If {
                        cond,
                        then,
                        otherwise,
                    } = &e.kind
                    {
                        self.gen_if(cond, then, otherwise.as_deref())?;
                    }
                }
            }
            if !self.terminated {
                self.emit(&format!("br label %{merge_l}"));
            }
            self.terminated
        } else {
            false // else なしは merge へ必ず到達する
        };

        // else あり・両分岐終端なら merge ブロックは到達不能。ラベルを出さない。
        let both_terminated = otherwise.is_some() && then_terminated && else_terminated;
        if both_terminated {
            self.terminated = true;
        } else {
            self.emit_label(&merge_l);
            self.terminated = false;
        }
        Ok(())
    }

    fn gen_block(&mut self, block: &Block) -> Result<(), CodegenError> {
        for stmt in &block.stmts {
            self.gen_stmt(stmt)?;
        }
        Ok(())
    }

    // ---- ヘルパ ---------------------------------------------------------

    fn lookup(&self, expr: &Expr) -> Result<(String, String), CodegenError> {
        self.res
            .uses
            .get(&expr.span)
            .and_then(|id| self.locals.get(id))
            .cloned()
            .ok_or_else(|| CodegenError::new(expr.span, "未対応の変数参照です"))
    }

    fn find_function(&self, name: &str) -> Option<&'a Function> {
        self.func_table.get(name).copied()
    }

    /// 式の LLVM 型を求める（型検査の結果から）。
    fn expr_llvm_ty(&self, expr: &Expr) -> Result<String, CodegenError> {
        let ty = self
            .types
            .get(&expr.span)
            .cloned()
            .unwrap_or(Ty::Infer)
            .defaulted();
        llvm_ty(&ty, self.structs).map_err(|m| CodegenError::new(expr.span, m))
    }

    /// 式の内部型（型検査の結果、リテラルは既定型へ確定）。
    fn iris_ty(&self, expr: &Expr) -> Ty {
        self.types
            .get(&expr.span)
            .cloned()
            .unwrap_or(Ty::Infer)
            .defaulted()
    }

    /// 式の内部型（型検査の結果のまま。リテラルは未確定型を保つ）。
    fn raw_ty(&self, expr: &Expr) -> Ty {
        let ty = self.types.get(&expr.span).cloned().unwrap_or(Ty::Infer);
        subst_ty(&ty, self.type_subst)
    }

    /// 式を評価し、期待型 `want` に合わせて必要なら暗黙デリファレンスして
    /// (値レジスタ, LLVM 型名) を返す。`want` が値型で式が参照型のときだけ
    /// `load` を挟んで指す先の値を取り出す（`want` が参照型・`Infer` なら参照のまま）。
    fn gen_value(&mut self, expr: &Expr, want: &Ty) -> Result<(String, String), CodegenError> {
        let want_value = !matches!(want, Ty::Infer | Ty::Error | Ty::Ref { .. });
        // リテラルの幅は期待型で決めたいので、値型を期待するときは want をヒントにする。
        let hint_ty = if want_value {
            want.clone()
        } else {
            self.raw_ty(expr).defaulted()
        };
        let hint = llvm_ty(&hint_ty, self.structs).map_err(|m| CodegenError::new(expr.span, m))?;
        let mut val = self.gen_expr(expr, &hint)?;

        // 参照の被演算子を、値型が期待される間だけ 1 段ずつ参照外しする。
        let mut cur = self.raw_ty(expr).defaulted();
        while let Ty::Ref { inner, .. } = &cur {
            if !want_value {
                break;
            }
            let inner_ty = (**inner).clone();
            let inner_ll = llvm_ty(&inner_ty, self.structs).map_err(|m| CodegenError::new(expr.span, m))?;
            let r = self.fresh_tmp();
            self.emit(&format!("{r} = load {inner_ll}, ptr {val}"));
            val = r;
            cur = inner_ty;
        }

        // 値型を期待し参照を外しきったときは want の幅で確定（リテラル対策）。
        let llty = if want_value && !matches!(cur, Ty::Ref { .. }) {
            llvm_ty(want, self.structs).map_err(|m| CodegenError::new(expr.span, m))?
        } else {
            llvm_ty(&cur, self.structs).map_err(|m| CodegenError::new(expr.span, m))?
        };
        Ok((val, llty))
    }

    /// 二項演算の被演算子型と数値クラス（リテラルでない側を優先）。
    /// 参照の被演算子は暗黙デリファレンスのため指す先の型で判定する。
    /// 戻り値は (被演算子の内部型, LLVM 型名, 数値クラス)。
    fn operand_ty(&self, lhs: &Expr, rhs: &Expr) -> Result<(Ty, String, NumKind), CodegenError> {
        let raw_l = self.raw_ty(lhs).peel_refs().clone();
        let raw_r = self.raw_ty(rhs).peel_refs().clone();
        let chosen = match (is_literal_ty(&raw_l), is_literal_ty(&raw_r)) {
            (true, false) => raw_r,
            (false, true) => raw_l,
            _ => raw_l.defaulted(),
        }
        .defaulted();
        let llty = llvm_ty(&chosen, self.structs).map_err(|m| CodegenError::new(lhs.span, m))?;
        Ok((chosen.clone(), llty, num_kind(&chosen)))
    }

    fn emit(&mut self, line: &str) {
        let _ = writeln!(self.body, "  {line}");
    }

    fn emit_label(&mut self, label: &str) {
        let _ = writeln!(self.body, "{label}:");
        self.terminated = false;
    }

    fn fresh_tmp(&mut self) -> String {
        let r = format!("%t{}", self.tmp);
        self.tmp += 1;
        r
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let l = format!("{prefix}.{}", self.label);
        self.label += 1;
        l
    }
}

/// 三項・短絡論理の分岐値。
enum Branch<'a> {
    Expr(&'a Expr),
    ConstBool(bool),
}

/// リテラルの未確定型か。
fn is_literal_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::IntLit | Ty::FloatLit)
}

/// 内部型を LLVM 型名へ変換する（数値プリミティブ・bool・参照・struct）。
fn llvm_ty(ty: &Ty, reg: &StructReg) -> Result<String, String> {
    match ty {
        Ty::IntLit => Ok("i32".to_string()),
        Ty::FloatLit => Ok("double".to_string()),
        // 参照は opaque ポインタ（指す先の型は命令側で扱う）。
        Ty::Ref { .. } => Ok("ptr".to_string()),
        Ty::Named { name, args } if args.is_empty() => match name.as_str() {
            // 整数は LLVM では符号を型に持たない（幅だけ）。符号は命令側で扱う。
            "i8" | "u8" => Ok("i8".to_string()),
            "i16" | "u16" => Ok("i16".to_string()),
            "i32" | "u32" => Ok("i32".to_string()),
            "i64" | "u64" => Ok("i64".to_string()),
            "f32" => Ok("float".to_string()),
            "f64" => Ok("double".to_string()),
            "bool" => Ok("i1".to_string()),
            // 文字列は NUL 終端の C 文字列へのポインタ（opaque ポインタ）。
            "string" => Ok("ptr".to_string()),
            "void" => Ok("void".to_string()),
            // 定義済み struct・enum（別名チェーン越しを含む）は名前付き構造体型。
            other => {
                if let Some((canon, _)) = reg.struct_def(other) {
                    Ok(format!("%{canon}"))
                } else if let Some(target) = reg.alias_to.get(other) {
                    // struct でない別名（`type Meters = f64` など）は元の型へ。
                    llvm_ty(&Ty::named(target), reg)
                } else {
                    // enum 型は `%Name = type { i8, i64 }` として宣言済み。
                    // 未知の型でも `%Name` として出力し、clang に任せる。
                    Ok(format!("%{other}"))
                }
            }
        },
        other => Err(format!(
            "型 `{}` のコード生成は未対応です（数値プリミティブ・bool・struct のみ）",
            other.describe()
        )),
    }
}

/// 名前付き型（引数なし）の名前を取り出す。参照は剥がす。struct 解決の入口に使う。
fn struct_name_of(ty: &Ty) -> Option<String> {
    match ty.peel_refs() {
        Ty::Named { name, args } if args.is_empty() => Some(name.clone()),
        _ => None,
    }
}

/// 命令選択のための数値クラス（LLVM 型は符号を持たないため別途必要）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumKind {
    SInt,
    UInt,
    Float,
}

/// 内部型から数値クラスを求める。整数・浮動小数以外は既定で符号付き整数扱い
/// （bool 等の `==`/`!=` は整数比較でよい）。
fn num_kind(ty: &Ty) -> NumKind {
    match ty {
        Ty::FloatLit => NumKind::Float,
        Ty::Named { name, args } if args.is_empty() => match name.as_str() {
            "u8" | "u16" | "u32" | "u64" => NumKind::UInt,
            "f32" | "f64" => NumKind::Float,
            _ => NumKind::SInt,
        },
        _ => NumKind::SInt,
    }
}

/// 浮動小数リテラルを LLVM が受け付ける定数表現（double のビット列）にする。
/// LLVM のテキスト IR では float 定数も「正確に表現できる double」のビット列で書く。
fn float_const(v: f64, llty: &str) -> String {
    let bits = if llty == "float" {
        ((v as f32) as f64).to_bits()
    } else {
        v.to_bits()
    };
    format!("0x{bits:016X}")
}

/// LLVM 型の零値（既定 return 用）。
fn zero_value(llty: &str) -> &'static str {
    match llty {
        "float" | "double" => "0.0",
        "ptr" => "null",
        // 名前付き struct 型（`%Name`）は zeroinitializer。
        _ if llty.starts_with('%') => "zeroinitializer",
        _ => "0",
    }
}
