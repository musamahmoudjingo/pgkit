use super::DatabaseTableColumn;
use crate::query_builder::{DeleteBuilder, InsertBuilder, SelectBuilder, UpdateBuilder};

/// A table in the the database.
pub trait DatabaseTable {
    /// The columns for this table.
    type Columns: DatabaseTableColumn;

    /// Database table name.
    const NAME: &'static str;

    /// Gets the primary key for this table.
    fn primary_key() -> Self::Columns;

    /// The soft-delete column (e.g. `deleted_at`) if this table has one.
    fn soft_delete_column() -> Option<Self::Columns> {
        None
    }

    /// Start a `SELECT …` statement from this table.
    fn select<'a>() -> SelectBuilder<'a> {
        SelectBuilder::new().from(Self::NAME)
    }

    /// Start an `INSERT …` statement into this table.
    fn insert<'a>() -> InsertBuilder<'a> {
        InsertBuilder::new().into(Self::NAME)
    }

    /// Start an `UPDATE …` statement on this table.
    fn update<'a>() -> UpdateBuilder<'a> {
        UpdateBuilder::new().table(Self::NAME)
    }

    /// Start a `DELETE …` statement from this table.
    fn delete<'a>() -> DeleteBuilder<'a> {
        DeleteBuilder::new().from(Self::NAME)
    }
}
