/// Range operators (e.g. `col BETWEEN a AND b`).
#[derive(Clone, Copy, Debug, strum::Display, strum::EnumString)]
pub enum RangeOperator {
    #[strum(to_string = " BETWEEN ", serialize = "BETWEEN")]
    Between,

    #[strum(to_string = " NOT BETWEEN ", serialize = "NOT BETWEEN")]
    NotBetween,
}
