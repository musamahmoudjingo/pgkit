use crate::ordering::OrderByDirection;

/// `NULLS FIRST` / `NULLS LAST` modifier.
#[derive(Clone, Copy)]
pub enum NullsOrder {
    First,
    Last,
}

impl std::fmt::Display for NullsOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::First => "NULLS FIRST",
            Self::Last => "NULLS LAST",
        })
    }
}

/// Postgres's default nulls placement for a given direction:
/// `ASC` ⇒ `NULLS LAST`, `DESC` ⇒ `NULLS FIRST`.
impl From<OrderByDirection> for NullsOrder {
    fn from(dir: OrderByDirection) -> Self {
        match dir {
            OrderByDirection::Asc => Self::Last,
            OrderByDirection::Desc => Self::First,
        }
    }
}
