/// Null-check operators (unary, e.g. `col IS NULL`).
#[derive(Clone, Copy, Debug, strum::Display, strum::EnumString)]
pub enum NullOperator {
    #[strum(to_string = " IS NULL", serialize = "IS NULL")]
    IsNull,

    #[strum(to_string = " IS NOT NULL", serialize = "IS NOT NULL")]
    IsNotNull,
}
