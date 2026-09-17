/// `UNION` flavor between two SELECTs.
#[derive(Clone, Copy)]
pub(super) enum UnionKind {
    Distinct,
    All,
}

impl UnionKind {
    pub(super) fn keyword(self) -> &'static str {
        match self {
            UnionKind::Distinct => " UNION ",
            UnionKind::All => " UNION ALL ",
        }
    }
}
