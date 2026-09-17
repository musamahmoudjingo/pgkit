/// Set membership operators (e.g. `col IN (...)`).
///
/// `Display` emits the operator with a leading and trailing space so it can be
/// dropped straight into a `sqlx::QueryBuilder::push` between a column and the
/// opening `(` of the value list.
#[derive(Clone, Copy, Debug, strum::Display, strum::EnumString)]
pub enum SetOperator {
    #[strum(to_string = " IN ", serialize = "IN")]
    In,

    #[strum(to_string = " NOT IN ", serialize = "NOT IN")]
    NotIn,
}
