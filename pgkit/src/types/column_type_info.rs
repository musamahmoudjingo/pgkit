//! `ColumnTypeInfo`: runtime access to a column's Postgres type.

/// Reports the Postgres type of each column variant.
pub trait ColumnTypeInfo {
    /// The Postgres type of this column, taken from the column's Rust type via
    /// [`sqlx::Type`]. For a Postgres `enum` column this is a
    /// `DeclareWithName` whose `name()` is the SQL type name (e.g.
    /// `"geo_service_status"`); for built-ins it is the corresponding builtin
    /// type (e.g. `Int4`, `Text`).
    fn pg_type_info(&self) -> sqlx::postgres::PgTypeInfo;
}
