//! モジュールローダー。
//!
//! `use a.b.c` 宣言をファイルシステム上のパス `a/b/c.iris` に対応させ、
//! ファイルを読み込み・字句解析・構文解析して pub アイテムを抽出する。
//!
//! - `std.*` パスは `CARGO_MANIFEST_DIR/std/` または実行ファイル隣の `std/` から解決する。
//! - ユーザーモジュールは指定のベースディレクトリから解決する。
//! - MVP 制約: モジュール自身が `use` 宣言を持つことはできない（解析エラーとはしないが無視する）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{Item, Program, UseDecl, UseTree};
use crate::lexer;
use crate::parser;

/// モジュールロード時のエラー。
#[derive(Debug, Clone)]
pub struct ModuleError {
    pub message: String,
}

impl ModuleError {
    fn new(msg: impl Into<String>) -> Self {
        ModuleError { message: msg.into() }
    }
}

/// ロード済みモジュールの情報。
pub struct LoadedModule {
    /// モジュールのソース文字列（診断で使う）。
    pub source: String,
    /// pub アイテムのみ。
    pub items: Vec<Item>,
    /// pub 関数名の集合（名前解決で使う）。
    pub pub_fn_names: Vec<String>,
    /// pub enum バリアント名の集合。
    pub pub_variant_names: Vec<String>,
}

/// モジュールローダー。モジュールパス → ロード結果のキャッシュを持つ。
pub struct ModuleLoader {
    /// キャッシュ: パスセグメント列 → ロード結果。
    modules: HashMap<Vec<String>, LoadedModule>,
    /// ユーザーソースのベースディレクトリ（`None` のときはカレントディレクトリ）。
    base_dir: Option<PathBuf>,
    /// std/ ディレクトリのパス。
    std_dir: PathBuf,
}

impl ModuleLoader {
    /// 新しいモジュールローダーを作る。
    ///
    /// `base_dir`: ユーザーモジュールの解決基準ディレクトリ（コンパイル対象ファイルの親）。
    pub fn new(base_dir: Option<&Path>) -> Self {
        ModuleLoader {
            modules: HashMap::new(),
            base_dir: base_dir.map(|p| p.to_path_buf()),
            std_dir: find_std_dir(),
        }
    }

    /// モジュールをロードする。既にキャッシュ済みなら返す。
    pub fn load(&mut self, path: &[String]) -> Result<&LoadedModule, ModuleError> {
        if self.modules.contains_key(path) {
            return Ok(&self.modules[path]);
        }
        let file_path = self.resolve_path(path);
        let source = std::fs::read_to_string(&file_path).map_err(|e| {
            ModuleError::new(format!(
                "モジュール `{}` を読み込めませんでした: {e} (パス: {})",
                path.join("."),
                file_path.display()
            ))
        })?;
        let loaded = compile_module_source(&source, path)?;
        self.modules.insert(path.to_vec(), loaded);
        Ok(&self.modules[path])
    }

    /// モジュールパスをファイルシステムのパスへ変換する。
    fn resolve_path(&self, path: &[String]) -> PathBuf {
        // `std` で始まるパスは std_dir から解決する。
        if path.first().map(|s| s == "std").unwrap_or(false) {
            let mut p = self.std_dir.clone();
            for seg in &path[1..] {
                p = p.join(seg);
            }
            p.set_extension("iris");
            return p;
        }
        // それ以外はベースディレクトリから解決する。
        let base = self
            .base_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let mut p = base;
        for seg in path {
            p = p.join(seg);
        }
        p.set_extension("iris");
        p
    }

    /// `use` 宣言群を処理し、インポートした pub アイテムを返す（prepend 用）。
    ///
    /// 戻り値は `(prepend_items, module_namespaces, errors)` のタプル:
    /// - `prepend_items`: main の program.items 先頭に挿入するアイテム列
    /// - `module_namespaces`: Plain import の場合のパス → pub 関数名マッピング（名前解決で使う）
    /// - `errors`: ロードエラーのリスト（致命的ではない場合に蓄積）
    pub fn process_use_decls(
        &mut self,
        decls: &[UseDecl],
    ) -> (Vec<Item>, HashMap<Vec<String>, Vec<String>>, Vec<ModuleError>) {
        let mut prepend: Vec<Item> = Vec::new();
        let mut namespaces: HashMap<Vec<String>, Vec<String>> = HashMap::new();
        let mut errors: Vec<ModuleError> = Vec::new();

        for decl in decls {
            let path = &decl.path;
            let loaded = match self.load(path) {
                Ok(m) => m,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };

            // codegen のためにすべての pub アイテムを常に prepend する。
            // Plain / Named / Glob の違いは「名前を直接スコープへ注入するか」だけ。
            prepend.extend(loaded.items.clone());

            match &decl.tree {
                UseTree::Plain => {
                    // `a.b.x(...)` 形式でのみアクセスできるよう名前空間を登録する。
                    // 直接スコープへの名前注入はしない。
                    namespaces.insert(path.clone(), loaded.pub_fn_names.clone());
                }
                UseTree::Named(_names) => {
                    // 将来: 指定した名前のみスコープへ注入する制限を実装する。
                    // MVP では全 pub 名が prepend 経由で利用可能になる（制限なし）。
                }
                UseTree::Glob => {
                    // すべての pub 名が prepend 経由で利用可能になる。
                }
            }
        }

        (prepend, namespaces, errors)
    }
}

/// std/ ディレクトリを探す。
///
/// 1. `IRIS_STD_DIR` 環境変数
/// 2. `CARGO_MANIFEST_DIR/std/`（cargo テスト・開発時）
/// 3. 実行ファイルの隣の `std/`
/// 4. カレントディレクトリの `std/`（最終フォールバック）
fn find_std_dir() -> PathBuf {
    if let Ok(d) = std::env::var("IRIS_STD_DIR") {
        return PathBuf::from(d);
    }
    if let Ok(d) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(d).join("std");
        if p.is_dir() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("std");
            if p.is_dir() {
                return p;
            }
        }
    }
    PathBuf::from("std")
}

/// モジュールソースを字句解析・構文解析し、pub アイテムを抽出する。
fn compile_module_source(source: &str, path: &[String]) -> Result<LoadedModule, ModuleError> {
    let tokens = lexer::lex(source).map_err(|e| {
        ModuleError::new(format!(
            "モジュール `{}` の字句解析エラー (offset {}): {}",
            path.join("."),
            e.offset,
            e.message
        ))
    })?;

    let program: Program = parser::parse(&tokens).map_err(|e| {
        ModuleError::new(format!(
            "モジュール `{}` の構文解析エラー: {:?}",
            path.join("."),
            e
        ))
    })?;

    extract_pub_items(source, program)
}

/// `Program` から pub アイテムを抽出して `LoadedModule` を作る。
fn extract_pub_items(source: &str, program: Program) -> Result<LoadedModule, ModuleError> {
    let mut items: Vec<Item> = Vec::new();
    let mut pub_fn_names: Vec<String> = Vec::new();
    let mut pub_variant_names: Vec<String> = Vec::new();

    for item in program.items {
        match &item {
            Item::Function(f) => {
                if f.is_pub || f.is_extern {
                    pub_fn_names.push(f.name.clone());
                    items.push(item);
                }
            }
            Item::TypeDef(t) => {
                if t.is_pub {
                    // pub enum のバリアント名も収集する。
                    if let crate::ast::TypeDefBody::Enum(variants) = &t.body {
                        for v in variants {
                            pub_variant_names.push(v.name.clone());
                        }
                    }
                    items.push(item);
                }
            }
            Item::Trait(tr) => {
                if tr.is_pub {
                    items.push(item);
                }
            }
            Item::Impl(_) => {
                // impl は型と紐づいているため、型が pub なら一緒に含める。
                // MVP では全 impl を含める。
                items.push(item);
            }
            Item::Use(_) => {
                // モジュール自身の use は MVP では無視する。
            }
        }
    }

    Ok(LoadedModule {
        source: source.to_string(),
        items,
        pub_fn_names,
        pub_variant_names,
    })
}
