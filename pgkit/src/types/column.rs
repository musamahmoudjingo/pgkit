use strum::{IntoEnumIterator, VariantNames};

use super::DatabaseTable;

/// Column of a database table.
pub trait DatabaseTableColumn: Copy + VariantNames + IntoEnumIterator + Into<&'static str> {
    /// The table this column belongs to.
    type Table: DatabaseTable;

    /// The primary key column.
    fn table_primary_key() -> Self;

    /// All columns as a comma-separated `SELECT` list, qualified by the
    /// table's name, e.g. `geo_countries.id, geo_countries.iso2`.
    fn select_all() -> String {
        let table = <Self::Table as DatabaseTable>::NAME;
        Self::iter()
            .map(|c| {
                let name: &'static str = c.into();
                format!("{table}.{name}")
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// All columns as a comma-separated `SELECT` list, qualified by the
    /// given table alias, e.g. `c.id, c.iso2`.
    fn select_all_with_table_alias(table_alias: &str) -> String {
        Self::iter()
            .map(|c| {
                let name: &'static str = c.into();
                format!("{table_alias}.{name}")
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}
