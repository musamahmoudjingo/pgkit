use sqlx::{Decode, Error, Row, Type, postgres::PgRow};

/// Decode a column whose presence in the row is optional.
///
/// - `Ok(Some(v))`: the column was selected and decoded successfully.
/// - `Ok(None)`: sqlx returned `ColumnNotFound`, the column was absent
///   from the projection.
/// - `Err(_)`: any other decode error.
///
/// To preserve the SQL-NULL distinction for nullable columns, callers pass
/// `Option<T>` as the type parameter and store the result as `Option<Option<T>>`. 
/// For non-nullable columns, pass `T` directly.
pub fn try_get_option<'r, T>(row: &'r PgRow, name: &str) -> sqlx::Result<Option<T>>
where
    T: Decode<'r, sqlx::Postgres> + Type<sqlx::Postgres>,
{
    match row.try_get::<T, _>(name) {
        Ok(v) => Ok(Some(v)),
        Err(Error::ColumnNotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}
