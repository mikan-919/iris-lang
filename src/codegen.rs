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
//! ローカルは alloca + load/store で扱う（SSA 化は LLVM の mem2reg に任せられる）。
//! enum・`!`・文字列・ジェネリクスなどは未対応（エラーにする）。参照越しの代入
//! （write-through `&mut x = v`）はまだ無く、参照への再代入は束縛の付け替えになる。

use std::collections::HashMap;
use std::fmt::Write;

use crate::ast::{
    BinaryOp, Block, Else, Expr, ExprKind, FieldInit, Function, Item, Program, Stmt, Type,
    TypeDefBody, UnaryOp,
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

    for item in &program.items {
        match item {
            Item::Function(f) => {
                let mut cg = FnCodegen {
                    res,
                    types: &type_info.expr_types,
                    def_spans: &def_spans,
                    func_table: &func_table,
                    structs: &structs,
                    body: String::new(),
                    tmp: 0,
                    label: 0,
                    locals: HashMap::new(),
                    terminated: false,
                    loops: Vec::new(),
                    ret_ty: Ty::unit(),
                };
                let func_ir = cg.emit_function(f)?;
                module.push_str(&func_ir);
                module.push('\n');
            }
            // 型定義はコード生成では型情報としてのみ使い、IR には出さない。
            Item::TypeDef(_) => {}
        }
    }
    Ok(module)
}

struct FnCodegen<'a> {
    res: &'a Resolution,
    types: &'a HashMap<Span, Ty>,
    def_spans: &'a HashMap<Span, DefId>,
    func_table: &'a HashMap<String, &'a Function>,
    structs: &'a StructReg,
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
}

impl<'a> FnCodegen<'a> {
    fn emit_function(&mut self, f: &Function) -> Result<String, CodegenError> {
        self.ret_ty = match &f.ret {
            Some(t) => Ty::from_ast(t),
            None => Ty::unit(),
        };
        let ret_ty = match &f.ret {
            Some(t) => llvm_ty(&Ty::from_ast(t), self.structs).map_err(|m| CodegenError::new(t.span(), m))?,
            None => "void".to_string(),
        };

        // extern は C 関数の宣言だけを出す。
        if f.is_extern {
            let mut tys = Vec::new();
            for p in &f.params {
                tys.push(
                    llvm_ty(&Ty::from_ast(&p.ty), self.structs).map_err(|m| CodegenError::new(p.span, m))?,
                );
            }
            return Ok(format!("declare {ret_ty} @{}({})\n", f.name, tys.join(", ")));
        }

        // 引数リスト。
        let mut params_sig = Vec::new();
        let mut param_setup = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            let ty = llvm_ty(&Ty::from_ast(&p.ty), self.structs)
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

        for stmt in &f.body.stmts {
            self.gen_stmt(stmt)?;
        }

        // 終端していなければ既定の return を補う。
        if !self.terminated {
            if ret_ty == "void" {
                self.emit("ret void");
            } else {
                self.emit(&format!("ret {ret_ty} {}", zero_value(&ret_ty)));
            }
        }

        let mut out = String::new();
        let _ = writeln!(
            out,
            "define {ret_ty} @{}({}) {{",
            f.name,
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
                    Some(t) => Ty::from_ast(t),
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
                let (ptr, llty) = self.place_ptr(target)?;
                let want = self.raw_ty(target).defaulted();
                let (v, _) = self.gen_value(value, &want)?;
                self.emit(&format!("store {llty} {v}, ptr {ptr}"));
                Ok(())
            }
            Stmt::While { cond, body, .. } => self.gen_while(cond, body),
            Stmt::Loop { body, .. } => self.gen_loop(body),
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
            ExprKind::Member { object, field } => {
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

    /// 式を評価し、結果の値（レジスタまたは定数）を返す。`hint` はリテラルの型。
    fn gen_expr(&mut self, expr: &Expr, hint: &str) -> Result<String, CodegenError> {
        match &expr.kind {
            ExprKind::Int(v) => Ok(v.to_string()),
            ExprKind::Float(v) => Ok(float_const(*v, hint)),
            ExprKind::Bool(b) => Ok(if *b { "1" } else { "0" }.to_string()),
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
            ExprKind::Member { object, field } => {
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
            _ => Err(CodegenError::new(
                expr.span,
                "この式はコード生成に未対応です",
            )),
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
        let ExprKind::Ident(name) = &callee.kind else {
            return Err(CodegenError::new(span, "この呼び出しはコード生成に未対応です"));
        };
        // 関数のシグネチャから引数型・戻り値型を引く。
        let func = self
            .find_function(name)
            .ok_or_else(|| CodegenError::new(span, format!("`{name}` の定義が見つかりません")))?;
        let ret_ty = match &func.ret {
            Some(t) => llvm_ty(&Ty::from_ast(t), self.structs).map_err(|m| CodegenError::new(t.span(), m))?,
            None => "void".to_string(),
        };
        let mut arg_strs = Vec::new();
        for (i, a) in args.iter().enumerate() {
            // 仮引数の型を期待型として渡す。引数が参照で仮引数が値型なら
            // gen_value が暗黙にデリファレンスする。
            let want = func
                .params
                .get(i)
                .map(|p| Ty::from_ast(&p.ty))
                .unwrap_or(Ty::Infer);
            let (v, pty) = self.gen_value(a, &want)?;
            arg_strs.push(format!("{pty} {v}"));
        }
        let call = format!("call {ret_ty} @{name}({})", arg_strs.join(", "));
        if ret_ty == "void" {
            self.emit(&call);
            Ok(String::new())
        } else {
            let r = self.fresh_tmp();
            self.emit(&format!("{r} = {call}"));
            Ok(r)
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
        if !self.terminated {
            self.emit(&format!("br label %{merge_l}"));
        }

        if let Some(els) = otherwise {
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
        }

        self.emit_label(&merge_l);
        self.terminated = false;
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
        self.types.get(&expr.span).cloned().unwrap_or(Ty::Infer)
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
            "void" => Ok("void".to_string()),
            // 定義済み struct（別名チェーン越しを含む）は名前付き構造体型。
            other => {
                if let Some((canon, _)) = reg.struct_def(other) {
                    Ok(format!("%{canon}"))
                } else if let Some(target) = reg.alias_to.get(other) {
                    // struct でない別名（`type Meters = f64` など）は元の型へ。
                    llvm_ty(&Ty::named(target), reg)
                } else {
                    Err(format!(
                        "型 `{other}` のコード生成は未対応です（数値プリミティブ・bool・struct のみ）"
                    ))
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
