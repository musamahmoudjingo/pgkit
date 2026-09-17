use super::row::{Binder, InsertRow};
use crate::types::ColumnRef;

/// Internal body shape of the INSERT.
pub enum InsertBody<'a> {
    /// Nothing pushed yet.
    Empty,
    /// `(col1, col2, …) VALUES ($1, $2, …)`: one row, paired column/value.
    Paired(Vec<(ColumnRef, Binder<'a>)>),
    /// `(col1, col2, …) VALUES (…), (…), …`: N rows of pre-declared columns.
    Bulk {
        columns: Vec<ColumnRef>,
        rows: Vec<InsertRow<'a>>,
    },
    /// `(col1, col2, …) <subquery>`: INSERT … SELECT.
    Select {
        columns: Vec<ColumnRef>,
        body: Binder<'a>,
    },
}
