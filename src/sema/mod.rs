//! 意味解析。AST に対して名前解決・型検査・所有権解析を行う。
//!
//! 縦切りで段階的に積み上げる。現状は **名前解決** ([`resolve`])、
//! **型検査** ([`typeck`])、**所有権（ムーブ）検査** ([`ownership`]) を実装済み。

pub mod ownership;
pub mod resolve;
pub mod ty;
pub mod typeck;

pub use ownership::{OwnershipError, check as check_ownership};
pub use resolve::{Def, DefId, DefKind, Resolution, ResolveError, resolve};
pub use ty::Ty;
pub use typeck::{TypeError, TypeInfo, check};
