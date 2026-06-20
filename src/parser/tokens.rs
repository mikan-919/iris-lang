//! 構文解析器の入力となるトークンストリーム。
//! `&[Token]` をラップし nom の `Input` トレイトを実装することで、
//! 字句解析が生成したトークン列の上で nom コンビネータを使えるようにする。

use std::iter::Enumerate;
use std::slice::Iter;

use nom::{Input, Needed};

use crate::span::Span;
use crate::token::{Token, TokenKind};

#[derive(Debug, Clone, Copy)]
pub struct Tokens<'a> {
    pub toks: &'a [Token],
}

impl<'a> Tokens<'a> {
    pub fn new(toks: &'a [Token]) -> Self {
        Tokens { toks }
    }

    /// 先頭トークン。字句解析器が必ず末尾に `Eof` を置くため、
    /// 解析中にスライスが空になることはない（`Eof` は消費しない）。
    pub fn first(&self) -> &'a Token {
        &self.toks[0]
    }

    /// 先頭トークンの種別。
    pub fn peek(&self) -> &'a TokenKind {
        &self.toks[0].kind
    }

    /// 診断に使う、現在位置の span。
    pub fn span(&self) -> Span {
        self.toks[0].span
    }
}

impl<'a> Input for Tokens<'a> {
    type Item = &'a Token;
    type Iter = Iter<'a, Token>;
    type IterIndices = Enumerate<Iter<'a, Token>>;

    fn input_len(&self) -> usize {
        self.toks.len()
    }

    fn take(&self, index: usize) -> Self {
        Tokens::new(&self.toks[0..index])
    }

    fn take_from(&self, index: usize) -> Self {
        Tokens::new(&self.toks[index..])
    }

    fn take_split(&self, index: usize) -> (Self, Self) {
        let (prefix, suffix) = self.toks.split_at(index);
        (Tokens::new(suffix), Tokens::new(prefix))
    }

    fn position<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::Item) -> bool,
    {
        self.toks.iter().position(predicate)
    }

    fn iter_elements(&self) -> Self::Iter {
        self.toks.iter()
    }

    fn iter_indices(&self) -> Self::IterIndices {
        self.toks.iter().enumerate()
    }

    fn slice_index(&self, count: usize) -> Result<usize, Needed> {
        if self.toks.len() >= count {
            Ok(count)
        } else {
            Err(Needed::new(count - self.toks.len()))
        }
    }
}
