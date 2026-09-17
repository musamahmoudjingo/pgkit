use super::{error::CursorPaginationError, value::CursorValue};

/// A row that can be/has been paginated/returned by [`super::CursorPagination`] .
pub trait PaginatedRow<I, C> {
    /// Returns the row's primary key.
    fn row_id(&self) -> Result<I, CursorPaginationError>;

    /// Returns the row's value for the ordering column, as a typed
    /// [`CursorValue`]. `Ok(None)` means the column was SQL `NULL`.
    fn ordering_column_value(
        &self,
        column: C,
    ) -> Result<Option<CursorValue>, CursorPaginationError>;
}
