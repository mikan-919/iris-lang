use std::collections::HashMap;
use crate::ast::{self, Expr, Stmt, Item, Type, BinOp, PipelineStep, Span};

/// 型変数の識別子
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub usize);

/// 型チェッカーの内部で使われるセマンティクス型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// 整数型
    Int,
    /// 浮動小数点型
    Float,
    /// 文字列型
    Str,
    /// 真偽値型
    Bool,
    /// 型変数（推論中に決定される型）
    Var(TypeVarId),
    /// 関数型 (引数リスト, 戻り値)
    Fn {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
    /// ジェネリック型 (例: List<T>)
    Generic {
        name: String,
        args: Vec<Ty>,
    },
    /// Trait型（能力制約、例: #Text）
    Trait(String),
    /// ユニオン型 (TypeScriptの A | B)
    Union(Vec<Ty>),
    /// インターセクション型 (合成能力、例: T: #Text && #Serializable)
    Intersection(Vec<Ty>),
    /// プレースホルダー・エラー用
    Unknown,
}

impl Ty {
    /// ユニオン型の簡素化 (重複除去など)
    pub fn make_union(tys: Vec<Ty>) -> Self {
        let mut flat = Vec::new();
        for ty in tys {
            match ty {
                Ty::Union(inner) => flat.extend(inner),
                _ => flat.push(ty),
            }
        }
        // 重複排除 (簡易版)
        let mut unique = Vec::new();
        for ty in flat {
            if !unique.contains(&ty) {
                unique.push(ty);
            }
        }
        if unique.len() == 1 {
            unique.remove(0)
        } else {
            Ty::Union(unique)
        }
    }
}

/// 型エラー
#[derive(Debug, Clone)]
pub enum TypeError {
    TypeMismatch { expected: Ty, found: Ty, span: Span },
    UndefinedVariable(String, Span),
    InfiniteType(TypeVarId, Ty, Span),
    NotAFunction(Ty, Span),
    ArgumentCountMismatch { expected: usize, found: usize, span: Span },
    AmbiguousFunction(String, Span),
}

impl TypeError {
    pub fn span(&self) -> Span {
        match self {
            TypeError::TypeMismatch { span, .. } => *span,
            TypeError::UndefinedVariable(_, span) => *span,
            TypeError::InfiniteType(_, _, span) => *span,
            TypeError::NotAFunction(_, span) => *span,
            TypeError::ArgumentCountMismatch { span, .. } => *span,
            TypeError::AmbiguousFunction(_, span) => *span,
        }
    }

    pub fn to_string_message(&self) -> String {
        match self {
            TypeError::TypeMismatch { expected, found, .. } => {
                format!("type mismatch: expected `{:?}`, found `{:?}`", expected, found)
            }
            TypeError::UndefinedVariable(name, _) => {
                format!("undefined variable `{}`", name)
            }
            TypeError::InfiniteType(id, ty, _) => {
                format!("infinite type resolved: Var({}) = {:?}", id.0, ty)
            }
            TypeError::NotAFunction(ty, _) => {
                format!("expected a function type, found `{:?}`", ty)
            }
            TypeError::ArgumentCountMismatch { expected, found, .. } => {
                format!("argument count mismatch: expected {}, found {}", expected, found)
            }
            TypeError::AmbiguousFunction(name, _) => {
                format!("ambiguous function resolution for `{}`", name)
            }
        }
    }
}

/// 型変数と解決された型のマッピング（単一化の結果を保持）
#[derive(Default, Clone)]
pub struct Substitution {
    map: HashMap<TypeVarId, Ty>,
}

impl Substitution {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    /// 型変数を再帰的に適用して、解決済みの具象型を展開する
    pub fn resolve(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(id) => {
                if let Some(resolved) = self.map.get(id) {
                    // 解決先がさらに型変数の場合もあるため、再帰的に解決する
                    self.resolve(resolved)
                } else {
                    ty.clone()
                }
            }
            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|p| self.resolve(p)).collect(),
                ret: Box::new(self.resolve(ret)),
            },
            Ty::Generic { name, args } => Ty::Generic {
                name: name.clone(),
                args: args.iter().map(|a| self.resolve(a)).collect(),
            },
            Ty::Union(tys) => Ty::make_union(tys.iter().map(|t| self.resolve(t)).collect()),
            Ty::Intersection(tys) => Ty::Intersection(tys.iter().map(|t| self.resolve(t)).collect()),
            _ => ty.clone(),
        }
    }

    pub fn insert(&mut self, id: TypeVarId, ty: Ty) {
        self.map.insert(id, ty);
    }
}

/// 型環境 (Type Environment)
pub struct TypeEnv {
    variables: Vec<HashMap<String, Ty>>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            variables: vec![HashMap::new()],
        }
    }

    pub fn enter_scope(&mut self) {
        self.variables.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        self.variables.pop();
    }

    pub fn insert(&mut self, name: String, ty: Ty) {
        if let Some(scope) = self.variables.last_mut() {
            scope.insert(name, ty);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Ty> {
        for scope in self.variables.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }
}

/// 型チェッカーのコンテキスト
pub struct TypeChecker {
    next_var_id: usize,
    pub subst: Substitution,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            next_var_id: 0,
            subst: Substitution::new(),
        }
    }

    /// 新しい型変数を生成する
    pub fn fresh_var(&mut self) -> Ty {
        let id = TypeVarId(self.next_var_id);
        self.next_var_id += 1;
        Ty::Var(id)
    }

    /// ASTの構文上の型をセマンティクス型に変換する
    pub fn convert_type(&mut self, ast_ty: &Type) -> Ty {
        match ast_ty {
            Type::Named(name) => match name.as_str() {
                "Int" | "Int32" | "Int64" | "Number" => Ty::Int,
                "Float" | "Float64" => Ty::Float,
                "String" => Ty::Str,
                "Bool" => Ty::Bool,
                _ => Ty::Generic {
                    name: name.clone(),
                    args: vec![],
                },
            },
            Type::Generic(name, args) => Ty::Generic {
                name: name.clone(),
                args: args.iter().map(|a| self.convert_type(a)).collect(),
            },
            Type::Trait(name) => Ty::Trait(name.clone()),
        }
    }

    /// Occurs Check: 型変数 `id` が型 `ty` の中に現れるか検証する (無限ループ防止)
    fn occurs_check(&self, id: TypeVarId, ty: &Ty) -> bool {
        match ty {
            Ty::Var(other_id) => {
                if id == *other_id {
                    return true;
                }
                if let Some(resolved) = self.subst.map.get(other_id) {
                    self.occurs_check(id, resolved)
                } else {
                    false
                }
            }
            Ty::Fn { params, ret } => {
                params.iter().any(|p| self.occurs_check(id, p)) || self.occurs_check(id, ret)
            }
            Ty::Generic { args, .. } => args.iter().any(|a| self.occurs_check(id, a)),
            Ty::Union(tys) | Ty::Intersection(tys) => tys.iter().any(|t| self.occurs_check(id, t)),
            _ => false,
        }
    }

    /// 単一化 (Unification): 2つの型を同一化する
    pub fn unify(&mut self, t1: &Ty, t2: &Ty, span: Span) -> Result<(), TypeError> {
        let t1 = self.subst.resolve(t1);
        let t2 = self.subst.resolve(t2);

        match (&t1, &t2) {
            (Ty::Var(id1), Ty::Var(id2)) if id1 == id2 => Ok(()),
            (Ty::Var(id), other) | (other, Ty::Var(id)) => {
                if self.occurs_check(*id, other) {
                    return Err(TypeError::InfiniteType(*id, other.clone(), span));
                }
                self.subst.insert(*id, other.clone());
                Ok(())
            }
            (Ty::Int, Ty::Int) => Ok(()),
            (Ty::Float, Ty::Float) => Ok(()),
            (Ty::Str, Ty::Str) => Ok(()),
            (Ty::Bool, Ty::Bool) => Ok(()),

            (
                Ty::Fn { params: p1, ret: r1 },
                Ty::Fn { params: p2, ret: r2 },
            ) => {
                if p1.len() != p2.len() {
                    return Err(TypeError::ArgumentCountMismatch {
                        expected: p1.len(),
                        found: p2.len(),
                        span,
                    });
                }
                for (a, b) in p1.iter().zip(p2.iter()) {
                    self.unify(a, b, span)?;
                }
                self.unify(r1, r2, span)
            }

            (
                Ty::Generic { name: n1, args: a1 },
                Ty::Generic { name: n2, args: a2 },
            ) => {
                if n1 != n2 || a1.len() != a2.len() {
                    return Err(TypeError::TypeMismatch { expected: t1, found: t2, span });
                }
                for (x, y) in a1.iter().zip(a2.iter()) {
                    self.unify(x, y, span)?;
                }
                Ok(())
            }

            _ => {
                if t1 == t2 {
                    Ok(())
                } else {
                    Err(TypeError::TypeMismatch { expected: t1, found: t2, span })
                }
            }
        }
    }

    /// 期待される型 `expected` に対する式 `expr` の型検査 (Check Mode)
    pub fn check(&mut self, env: &mut TypeEnv, expr: &Expr, expected: &Ty, span: Span) -> Result<(), TypeError> {
        let expected = self.subst.resolve(expected);
        match (expr, &expected) {
            // 例: ラムダ式のチェック（Contextual Typing）
            (Expr::Lambda { params, body }, Ty::Fn { params: expected_params, ret: expected_ret }) => {
                if params.len() != expected_params.len() {
                    return Err(TypeError::ArgumentCountMismatch {
                        expected: expected_params.len(),
                        found: params.len(),
                        span,
                    });
                }
                env.enter_scope();
                for (param, expected_ty) in params.iter().zip(expected_params.iter()) {
                    // ラムダパラメータにアノテーションがあればそれと一致させる
                    let param_ty = if let Some(ref ast_ty) = param.ty {
                        let ty = self.convert_type(ast_ty);
                        self.unify(&ty, expected_ty, span)?;
                        ty
                    } else {
                        expected_ty.clone()
                    };
                    env.insert(param.name.clone(), param_ty);
                }
                self.check(env, body, expected_ret, span)?;
                env.exit_scope();
                Ok(())
            }
            // 通常の推論と単一化へのフォールバック
            _ => {
                let inferred = self.infer(env, expr, span)?;
                self.unify(&expected, &inferred, span)
            }
        }
    }

    /// 式 `expr` の型推論 (Infer Mode)
    pub fn infer(&mut self, env: &mut TypeEnv, expr: &Expr, fallback_span: Span) -> Result<Ty, TypeError> {
        match expr {
            Expr::Int(_) => Ok(Ty::Int),
            Expr::Float(_) => Ok(Ty::Float),
            Expr::Str(_) => Ok(Ty::Str),
            Expr::Bool(_) => Ok(Ty::Bool),
            Expr::Ident(name) => {
                if let Some(ty) = env.lookup(name) {
                    Ok(ty.clone())
                } else {
                    Err(TypeError::UndefinedVariable(name.clone(), fallback_span))
                }
            }
            Expr::BinOp { op, lhs, rhs } => {
                let lhs_ty = self.infer(env, lhs, fallback_span)?;
                let rhs_ty = self.infer(env, rhs, fallback_span)?;
                self.unify(&lhs_ty, &rhs_ty, fallback_span)?;
                
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        Ok(lhs_ty)
                    }
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        Ok(Ty::Bool)
                    }
                }
            }
            Expr::Call { func, args } => {
                let func_ty = self.infer(env, func, fallback_span)?;
                let func_ty = self.subst.resolve(&func_ty);

                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.infer(env, arg, fallback_span)?);
                }

                let ret_ty = self.fresh_var();
                let expected_fn = Ty::Fn {
                    params: arg_tys,
                    ret: Box::new(ret_ty.clone()),
                };

                self.unify(&func_ty, &expected_fn, fallback_span)?;
                Ok(self.subst.resolve(&ret_ty))
            }
            // パイプライン式
            Expr::Pipeline(pipeline) => {
                let pipeline_span = pipeline.span;
                let mut current_ty = self.infer(env, &pipeline.subject, pipeline_span)?;

                for (_op, step) in &pipeline.steps {
                    current_ty = self.infer_pipeline_step(env, &current_ty, step, pipeline_span)?;
                }

                Ok(current_ty)
            }
            // ラムダ式の推論（アノテーションがない場合は型変数を割り振る）
            Expr::Lambda { params, body } => {
                env.enter_scope();
                let mut param_tys = Vec::new();
                for param in params {
                    let param_ty = if let Some(ref ast_ty) = param.ty {
                        self.convert_type(ast_ty)
                    } else {
                        self.fresh_var()
                    };
                    env.insert(param.name.clone(), param_ty.clone());
                    param_tys.push(param_ty);
                }
                let ret_ty = self.infer(env, body, fallback_span)?;
                env.exit_scope();

                Ok(Ty::Fn {
                    params: param_tys,
                    ret: Box::new(ret_ty),
                })
            }
            // プレースホルダー型
            _ => Ok(self.fresh_var()),
        }
    }

    /// パイプラインの各ステップにおける型推論
    fn infer_pipeline_step(&mut self, env: &mut TypeEnv, subject_ty: &Ty, step: &PipelineStep, span: Span) -> Result<Ty, TypeError> {
        let subject_ty = self.subst.resolve(subject_ty);

        match step {
            PipelineStep::BinOpRhs { op, rhs } => {
                // 二項演算子の右辺。左辺（主体）と右辺の型を推論して単一化する
                let rhs_ty = self.infer(env, rhs, span)?;
                self.unify(&subject_ty, &rhs_ty, span)?;

                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        Ok(subject_ty)
                    }
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        Ok(Ty::Bool)
                    }
                }
            }
            PipelineStep::ImplicitCall { func: func_name, args } => {
                // 主体が自動的に第1引数になる: func(subject, ...args)
                let func_ty = if let Some(ty) = env.lookup(func_name) {
                    ty.clone()
                } else {
                    return Err(TypeError::UndefinedVariable(func_name.clone(), span));
                };
                let func_ty = self.subst.resolve(&func_ty);

                let mut arg_tys = vec![subject_ty];
                for arg in args {
                    arg_tys.push(self.infer(env, arg, span)?);
                }

                let ret_ty = self.fresh_var();
                let expected_fn = Ty::Fn {
                    params: arg_tys,
                    ret: Box::new(ret_ty.clone()),
                };

                self.unify(&func_ty, &expected_fn, span)?;
                Ok(self.subst.resolve(&ret_ty))
            }
            PipelineStep::ExplicitCall { func: func_name, args } => {
                // $ が主体の位置を表す
                let func_ty = if let Some(ty) = env.lookup(func_name) {
                    ty.clone()
                } else {
                    return Err(TypeError::UndefinedVariable(func_name.clone(), span));
                };
                let func_ty = self.subst.resolve(&func_ty);

                let mut arg_tys = Vec::new();
                for arg in args {
                    match arg {
                        ast::PlaceholderArg::Dollar => {
                            arg_tys.push(subject_ty.clone());
                        }
                        ast::PlaceholderArg::Expr(expr) => {
                            arg_tys.push(self.infer(env, &expr, span)?);
                        }
                    }
                }

                let ret_ty = self.fresh_var();
                let expected_fn = Ty::Fn {
                    params: arg_tys,
                    ret: Box::new(ret_ty.clone()),
                };

                self.unify(&func_ty, &expected_fn, span)?;
                Ok(self.subst.resolve(&ret_ty))
            }
            _ => Ok(self.fresh_var()),
        }
    }

    /// プログラム全体の型チェックを行い、型環境を構築する
    pub fn check_program(&mut self, env: &mut TypeEnv, program: &[Item]) -> Result<(), TypeError> {
        for item in program {
            self.check_item(env, item)?;
        }
        Ok(())
    }

    /// 各トップレベル宣言 (Item) の型チェック
    pub fn check_item(&mut self, env: &mut TypeEnv, item: &Item) -> Result<(), TypeError> {
        match item {
            Item::LetStmt(let_stmt) => {
                let let_span = let_stmt.span;
                let inferred_ty = self.infer(env, &let_stmt.value, let_span)?;
                let resolved_ty = if let Some(ref ast_ty) = let_stmt.ty {
                    let expected_ty = self.convert_type(ast_ty);
                    self.unify(&expected_ty, &inferred_ty, let_span)?;
                    expected_ty
                } else {
                    inferred_ty
                };
                let resolved_ty = self.subst.resolve(&resolved_ty);
                env.insert(let_stmt.name.clone(), resolved_ty);
                Ok(())
            }
            Item::FuncDef(func_def) => {
                let func_span = func_def.span;
                // 関数宣言から関数型を構築する
                let param_tys: Vec<Ty> = func_def.params.iter()
                    .map(|p| self.convert_type(&p.ty))
                    .collect();
                let ret_ty = self.convert_type(&func_def.ret_ty);

                let fn_ty = Ty::Fn {
                    params: param_tys.clone(),
                    ret: Box::new(ret_ty.clone()),
                };

                // 関数自体を環境に登録 (再帰呼び出し可能にするため)
                env.insert(func_def.name.clone(), fn_ty);

                // 関数内部スコープでパラメータを登録してボディをチェック
                env.enter_scope();
                for (param, ty) in func_def.params.iter().zip(param_tys.iter()) {
                    env.insert(param.name.clone(), ty.clone());
                }

                match &func_def.body {
                    ast::FuncBody::Block(stmts) => {
                        for stmt in stmts {
                            match stmt {
                                Stmt::Let(let_stmt) => {
                                    let let_span = let_stmt.span;
                                    let inferred = self.infer(env, &let_stmt.value, let_span)?;
                                    let resolved = if let Some(ref ast_ty) = let_stmt.ty {
                                        let expected = self.convert_type(ast_ty);
                                        self.unify(&expected, &inferred, let_span)?;
                                        expected
                                    } else {
                                        inferred
                                    };
                                    env.insert(let_stmt.name.clone(), self.subst.resolve(&resolved));
                                }
                                Stmt::Expr(expr) => {
                                    self.infer(env, expr, func_span)?;
                                }
                                Stmt::Item(item) => {
                                    self.check_item(env, item)?;
                                }
                            }
                        }
                    }
                    ast::FuncBody::Pipeline(pipeline) => {
                        let pipeline_expr = Expr::Pipeline(Box::new(pipeline.clone()));
                        self.check(env, &pipeline_expr, &ret_ty, func_span)?;
                    }
                }
                env.exit_scope();
                Ok(())
            }
            _ => {
                // その他の Item は一旦スルー
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::LambdaParam;

    #[test]
    fn test_literal_inference() {
        let mut checker = TypeChecker::new();
        let mut env = TypeEnv::new();

        assert_eq!(checker.infer(&mut env, &Expr::Int(42), (0, 0)).unwrap(), Ty::Int);
        assert_eq!(checker.infer(&mut env, &Expr::Str("hello".to_string()), (0, 0)).unwrap(), Ty::Str);
        assert_eq!(checker.infer(&mut env, &Expr::Bool(true), (0, 0)).unwrap(), Ty::Bool);
    }

    #[test]
    fn test_identity_lambda_inference() {
        let mut checker = TypeChecker::new();
        let mut env = TypeEnv::new();

        // (x -> x)
        let lambda = Expr::Lambda {
            params: vec![LambdaParam {
                name: "x".to_string(),
                ty: None,
            }],
            body: Box::new(Expr::Ident("x".to_string())),
        };

        let ty = checker.infer(&mut env, &lambda, (0, 0)).unwrap();
        // 推論された型は Fn { params: [Var(0)], ret: Var(0) } になるはず
        if let Ty::Fn { params, ret } = checker.subst.resolve(&ty) {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0], *ret); // 引数と戻り値が同じ型変数であること
        } else {
            panic!("Expected function type");
        }
    }

    #[test]
    fn test_bidirectional_checking() {
        let mut checker = TypeChecker::new();
        let mut env = TypeEnv::new();

        // 期待される型: Fn(Int) -> Int
        let expected_ty = Ty::Fn {
            params: vec![Ty::Int],
            ret: Box::new(Ty::Int),
        };

        // ラムダ式 (x -> x) (型アノテーションなし)
        let lambda = Expr::Lambda {
            params: vec![LambdaParam {
                name: "x".to_string(),
                ty: None,
            }],
            body: Box::new(Expr::Ident("x".to_string())),
        };

        // 検査モード (check) で検証する
        checker.check(&mut env, &lambda, &expected_ty, (0, 0)).unwrap();

        // 検査後、型変数が Int に解決されているか確認
        let resolved = checker.subst.resolve(&expected_ty);
        assert_eq!(
            resolved,
            Ty::Fn {
                params: vec![Ty::Int],
                ret: Box::new(Ty::Int),
            }
        );
    }
}
