//! ソース上の位置情報。バイトオフセットと長さで表す。
//! miette の `SourceSpan` へ変換して診断に利用する。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
}

impl Span {
    pub fn new(offset: usize, len: usize) -> Self {
        Span { offset, len }
    }

    pub fn end(self) -> usize {
        self.offset + self.len
    }

    /// 2 つの span を包含する最小の span を返す。
    pub fn merge(self, other: Span) -> Span {
        let start = self.offset.min(other.offset);
        let end = self.end().max(other.end());
        Span::new(start, end - start)
    }
}

impl From<Span> for miette::SourceSpan {
    fn from(span: Span) -> Self {
        miette::SourceSpan::new(span.offset.into(), span.len)
    }
}
