use crate::ordering::OrderByDirection;

/// Binary comparison operators (e.g. `col = val`).
#[derive(Clone, Copy, Debug, strum::Display, strum::EnumString)]
pub enum ComparisonOperator {
    #[strum(to_string = " = ", serialize = "=")]
    Equal,

    #[strum(to_string = " <> ", serialize = "<>")]
    NotEqual,

    #[strum(to_string = " > ", serialize = ">")]
    GreaterThan,

    #[strum(to_string = " < ", serialize = "<")]
    LessThan,

    #[strum(to_string = " >= ", serialize = ">=")]
    GreaterThanOrEqual,

    #[strum(to_string = " <= ", serialize = "<=")]
    LessThanOrEqual,
}

impl From<OrderByDirection> for ComparisonOperator {
    fn from(sort_dir: OrderByDirection) -> Self {
        match sort_dir {
            OrderByDirection::Asc => ComparisonOperator::GreaterThan,
            OrderByDirection::Desc => ComparisonOperator::LessThan,
        }
    }
}
