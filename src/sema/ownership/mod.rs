//! 所有権解析。仕様の「所有権 DAG」（`docs/spec/ownership.md`,
//! `compiler.md` の Open Question）を二層で検査する。
//!
//! 1. [`typegraph`] — **型レベルの所有権グラフ**。型同士の所有関係を有向グラフに
//!    して、所有のサイクル（＝無限サイズ型）を検出する。`&T`（非所有）や
//!    `Box`/`Vec`/`Map`/`Set`（ヒープ間接）は所有辺を作らないためサイクル可。
//! 2. [`flow`] — **値レベルのムーブ＋借用グラフ**。各関数を前進フロー解析し、
//!    ムーブ／use-after-move を検査するとともに、参照の出所（provenance）を
//!    グラフとして追跡してライフタイムの不正（ローカルを指す参照の返却＝
//!    ダングリング、指す先がムーブされた参照の使用）を検出する。
//!
//! まだ扱わないもの: 借用の競合（同時 `&mut`／共有中の可変借用などのエイリアス
//! 規則）、参照を返却以外の長寿命の場所へ格納する場合の領域検査、ループ内のムーブ。

mod borrows;
mod flow;
mod typegraph;

use crate::sema::resolve::Resolution;
use crate::sema::typeck::TypeInfo;
use crate::span::Span;
use crate::ast::Program;

/// 所有権エラー。`secondary` は補助ラベル（ムーブ位置・参照先の宣言位置など）。
#[derive(Debug, Clone)]
pub struct OwnershipError {
    pub span: Span,
    pub message: String,
    pub secondary: Option<(Span, String)>,
}

impl OwnershipError {
    fn new(span: Span, message: String) -> Self {
        OwnershipError {
            span,
            message,
            secondary: None,
        }
    }

    fn with_secondary(span: Span, message: String, sec_span: Span, sec_msg: String) -> Self {
        OwnershipError {
            span,
            message,
            secondary: Some((sec_span, sec_msg)),
        }
    }
}

/// プログラムの所有権検査を行う。
pub fn check(
    program: &Program,
    res: &Resolution,
    type_info: &TypeInfo,
) -> Result<(), Vec<OwnershipError>> {
    let mut errors = Vec::new();
    typegraph::check_cycles(program, &mut errors);
    flow::check_functions(program, res, type_info, &mut errors);
    borrows::check_borrows(program, res, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
