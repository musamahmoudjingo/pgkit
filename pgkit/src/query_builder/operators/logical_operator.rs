/// Logical connectives that combine boolean expressions.
#[derive(Clone, Copy, Debug, strum::Display, strum::EnumString)]
pub enum LogicalOperator {
    #[strum(to_string = " AND ", serialize = "AND")]
    And,

    #[strum(to_string = " OR ", serialize = "OR")]
    Or,
}
