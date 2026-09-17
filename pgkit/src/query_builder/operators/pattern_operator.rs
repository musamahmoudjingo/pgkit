/// Pattern-matching operators (e.g. `col LIKE 'foo%'`).
#[derive(Clone, Copy, Debug, strum::Display, strum::EnumString)]
pub enum PatternOperator {
    #[strum(to_string = " LIKE ", serialize = "LIKE")]
    Like,

    #[strum(to_string = " ILIKE ", serialize = "ILIKE")]
    ILike,
}
