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
//!
//! ローカルは alloca + load/store で扱う（SSA 化は LLVM の mem2reg に任せられる）。
//! struct・参照・`!`・文字列・ジェネリクスなどは未対応（エラーにする）。

use std::collections::HashMap;
use std::fmt::Write;

use crate::ast::{
    BinaryOp, Block, Else, Expr, ExprKind, Function, Item, Program, Stmt, UnaryOp,
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

    let mut module = String::from("; iris-lang が生成した LLVM IR\n\n");
    for item in &program.items {
        match item {
            Item::Function(f) => {
                let mut cg = FnCodegen {
                    res,
                    types: &type_info.expr_types,
                    def_spans: &def_spans,
                    func_table: &func_table,
                    body: String::new(),
                    tmp: 0,
                    label: 0,
                    locals: HashMap::new(),
                    terminated: false,
                    loops: Vec::new(),
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
    body: String,
    tmp: usize,
    label: usize,
    /// 束縛 DefId → (ポインタレジスタ, LLVM 型)。
    locals: HashMap<DefId, (String, String)>,
    terminated: bool,
    /// ネスト中のループの (continue 先ラベル, break 先ラベル) のスタック。
    loops: Vec<(String, String)>,
}

impl<'a> FnCodegen<'a> {
    fn emit_function(&mut self, f: &Function) -> Result<String, CodegenError> {
        let ret_ty = match &f.ret {
            Some(t) => llvm_ty(&Ty::from_ast(t)).map_err(|m| CodegenError::new(t.span(), m))?,
            None => "void".to_string(),
        };

        // extern は C 関数の宣言だけを出す。
        if f.is_extern {
            let mut tys = Vec::new();
            for p in &f.params {
                tys.push(
                    llvm_ty(&Ty::from_ast(&p.ty)).map_err(|m| CodegenError::new(p.span, m))?,
                );
            }
            return Ok(format!("declare {ret_ty} @{}({})\n", f.name, tys.join(", ")));
        }

        // 引数リスト。
        let mut params_sig = Vec::new();
        let mut param_setup = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            let ty = llvm_ty(&Ty::from_ast(&p.ty))
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
                let llty = match ty {
                    Some(t) => {
                        llvm_ty(&Ty::from_ast(t)).map_err(|m| CodegenError::new(t.span(), m))?
                    }
                    None => self.expr_llvm_ty(value)?,
                };
                let v = self.gen_expr(value, &llty)?;
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
                        let llty = self.expr_llvm_ty(v)?;
                        let r = self.gen_expr(v, &llty)?;
                        self.emit(&format!("ret {llty} {r}"));
                    }
                    None => self.emit("ret void"),
                }
                self.terminated = true;
                Ok(())
            }
            Stmt::Assign { target, value, .. } => {
                let (ptr, llty) = self.place_ptr(target)?;
                let v = self.gen_expr(value, &llty)?;
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
        let c = self.gen_expr(cond, "i1")?;
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
        if let ExprKind::Ident(_) = &target.kind
            && let Some(&id) = self.res.uses.get(&target.span)
            && let Some((ptr, ty)) = self.locals.get(&id)
        {
            return Ok((ptr.clone(), ty.clone()));
        }
        Err(CodegenError::new(
            target.span,
            "この代入先はコード生成に未対応です",
        ))
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
                let ity = self.iris_ty(inner);
                let ty = llvm_ty(&ity).map_err(|m| CodegenError::new(inner.span, m))?;
                let v = self.gen_expr(inner, &ty)?;
                let r = self.fresh_tmp();
                if num_kind(&ity) == NumKind::Float {
                    self.emit(&format!("{r} = fneg {ty} {v}"));
                } else {
                    self.emit(&format!("{r} = sub {ty} 0, {v}"));
                }
                Ok(r)
            }
            UnaryOp::Ref | UnaryOp::RefMut => {
                Err(CodegenError::new(span, "参照のコード生成は未対応です"))
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

        let (opnd_ty, kind) = self.operand_ty(lhs, rhs)?;
        let l = self.gen_expr(lhs, &opnd_ty)?;
        let r = self.gen_expr(rhs, &opnd_ty)?;
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
            Some(t) => llvm_ty(&Ty::from_ast(t)).map_err(|m| CodegenError::new(t.span(), m))?,
            None => "void".to_string(),
        };
        let mut arg_strs = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let pty = func
                .params
                .get(i)
                .map(|p| llvm_ty(&Ty::from_ast(&p.ty)))
                .transpose()
                .map_err(|m| CodegenError::new(a.span, m))?
                .unwrap_or_else(|| "i32".to_string());
            let v = self.gen_expr(a, &pty)?;
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
        let c = self.gen_expr(cond, "i1")?;
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
        let c = self.gen_expr(cond, "i1")?;
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
        llvm_ty(&ty).map_err(|m| CodegenError::new(expr.span, m))
    }

    /// 式の内部型（型検査の結果、リテラルは既定型へ確定）。
    fn iris_ty(&self, expr: &Expr) -> Ty {
        self.types
            .get(&expr.span)
            .cloned()
            .unwrap_or(Ty::Infer)
            .defaulted()
    }

    /// 二項演算の被演算子型と数値クラス（リテラルでない側を優先）。
    fn operand_ty(&self, lhs: &Expr, rhs: &Expr) -> Result<(String, NumKind), CodegenError> {
        let raw_l = self.types.get(&lhs.span).cloned().unwrap_or(Ty::Infer);
        let raw_r = self.types.get(&rhs.span).cloned().unwrap_or(Ty::Infer);
        let chosen = match (is_literal_ty(&raw_l), is_literal_ty(&raw_r)) {
            (true, false) => raw_r,
            (false, true) => raw_l,
            _ => raw_l.defaulted(),
        }
        .defaulted();
        let llty = llvm_ty(&chosen).map_err(|m| CodegenError::new(lhs.span, m))?;
        Ok((llty, num_kind(&chosen)))
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

/// 内部型を LLVM 型名へ変換する（数値プリミティブと bool）。
fn llvm_ty(ty: &Ty) -> Result<String, String> {
    match ty {
        Ty::IntLit => Ok("i32".to_string()),
        Ty::FloatLit => Ok("double".to_string()),
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
            other => Err(format!(
                "型 `{other}` のコード生成は未対応です（数値プリミティブと bool のみ）"
            )),
        },
        other => Err(format!(
            "型 `{}` のコード生成は未対応です（数値プリミティブと bool のみ）",
            other.describe()
        )),
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
        _ => "0",
    }
}
