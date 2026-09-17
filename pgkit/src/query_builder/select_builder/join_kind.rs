/// SQL join flavor.
#[derive(Clone, Copy)]
pub enum JoinKind {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

impl JoinKind {
    pub(super) fn keyword(self) -> &'static str {
        match self {
            JoinKind::Inner => "INNER JOIN",
            JoinKind::Left => "LEFT JOIN",
            JoinKind::Right => "RIGHT JOIN",
            JoinKind::Full => "FULL JOIN",
            JoinKind::Cross => "CROSS JOIN",
        }
    }
}
