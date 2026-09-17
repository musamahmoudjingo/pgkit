use std::cmp::PartialEq;

use serde::de::DeserializeOwned;

use sqlx::TypeInfo;

use super::super::defaults::MAX_PAGE_SIZE;
use super::cursor_error::CursorError;
use super::filters_signature::compute_filters_signature;
use super::{PaginatedRow, error::CursorPaginationError, payload::CursorPayload};
use crate::{
    ordering::{OrderBy, OrderByDirection},
    projection::ColumnSelection,
    types::ColumnTypeInfo,
};

/// Default cursor pagination limit.
const DEFAULT_CURSOR_LIMIT: u32 = 20;

/// Return type of [`CursorPagination::into_response_parts`].
pub type ResponseParts<R, T, Id> =
    Result<(Vec<R>, Option<CursorPayload<T, Id>>), CursorPaginationError>;

/// Cursor (keyset) pagination.
///
/// Use [`fetch_limit`](Self::fetch_limit) when issuing the SQL query: it
/// returns `effective_limit + 1` to detect whether next page exists.
///
/// Generic over `T` (sorting columns) and `Id` (primary key type).
#[derive(Debug, Clone)]
pub struct CursorPagination<T, Id = uuid::Uuid> {
    ordering: OrderBy<T>,
    limit: u32,
    cursor: Option<CursorPayload<T, Id>>,
    filter_sig: Option<String>,
}

impl<T, Id> CursorPagination<T, Id> {
    /// Constructs a [`CursorPagination`] validating that the
    /// ordering column is included in the projection.
    pub fn try_new<F>(
        ordering: OrderBy<T>,
        limit: u32,
        column_selection: &ColumnSelection<T>,
        cursor: Option<String>,
        filters: Option<&F>,
    ) -> Result<Self, CursorPaginationError>
    where
        T: Copy + DeserializeOwned + PartialEq + ColumnTypeInfo + std::fmt::Display,
        Id: DeserializeOwned,
        F: serde::Serialize + ?Sized,
    {
        let ordering_column = ordering.column();
        if !column_selection.contains(&ordering_column) {
            return Err(CursorPaginationError::OrderingColumnNotProjected {
                column: ordering_column.to_string(),
            });
        }
        Self::with_validated_column_selection(ordering, limit, cursor, filters)
    }

    /// Constructs a [`CursorPagination`] assuming the ordering column
    /// was already found in the selected columns.
    pub fn with_validated_column_selection<F>(
        ordering: OrderBy<T>,
        limit: u32,
        cursor: Option<String>,
        filters: Option<&F>,
    ) -> Result<Self, CursorPaginationError>
    where
        T: Copy + DeserializeOwned + PartialEq + ColumnTypeInfo,
        Id: DeserializeOwned,
        F: serde::Serialize + ?Sized,
    {
        let limit = if limit == 0 {
            DEFAULT_CURSOR_LIMIT
        } else {
            limit
        };

        let filter_sig = match filters {
            Some(f) => Some(compute_filters_signature(f)?),
            None => None,
        };

        let cursor = match cursor {
            Some(raw) => Self::reconcile(&raw, &ordering, filter_sig.as_deref())?,
            None => None,
        };

        Ok(Self {
            cursor,
            limit,
            ordering,
            filter_sig,
        })
    }

    fn reconcile(
        cursor: &str,
        ordering: &OrderBy<T>,
        filter_sig: Option<&str>,
    ) -> Result<Option<CursorPayload<T, Id>>, CursorPaginationError>
    where
        T: Copy + DeserializeOwned + PartialEq + ColumnTypeInfo,
        Id: DeserializeOwned,
    {
        let decoded = CursorPayload::<T, Id>::decode(cursor)?;

        if ordering.column() != decoded.col {
            return Ok(None);
        }

        if ordering.direction() != decoded.dir {
            return Ok(None);
        }

        if decoded.filter_sig.as_deref() != filter_sig {
            return Ok(None);
        }

        // Unlike the column/direction/filter mismatches above, which a client
        // reaches legitimately by changing the listing and keeping the cursor,
        // and which restart it at page one, a value the sort column cannot
        // hold is a cursor this library could not have minted. Binding it makes
        // PostgreSQL fail with a cast or missing-operator error, so reject it
        // here as the malformed cursor it is.
        if let Some(val) = &decoded.val {
            let column_type = ordering.column().pg_type_info();
            if !val.fits_column(&column_type) {
                return Err(CursorError::ValueTypeMismatch {
                    value: val.kind_name(),
                    column: column_type.name().to_string(),
                }
                .into());
            }
        }

        Ok(Some(decoded))
    }

    /// Clamped limit.
    pub fn effective_limit(&self) -> u32 {
        self.limit.min(MAX_PAGE_SIZE)
    }

    /// `effective_limit + 1`, for detecting whether next page exists.
    pub fn fetch_limit(&self) -> i64 {
        (self.effective_limit() as i64) + 1
    }

    /// Column and direction for ordering.
    pub fn order_by(&self) -> (T, OrderByDirection)
    where
        T: Copy,
    {
        (self.ordering.column(), self.ordering.direction())
    }

    /// Returns the decoded cursor payload, if a cursor was provided and successfully reconciled.
    pub fn cursor(&self) -> Option<&CursorPayload<T, Id>> {
        self.cursor.as_ref()
    }

    /// Signature of the filters.
    pub fn filter_sig(&self) -> Option<&str> {
        self.filter_sig.as_deref()
    }

    /// Mints the next-page cursor if `rows` is longer than
    /// [`effective_limit`](Self::effective_limit) and truncates
    /// `rows` to [`effective_limit`](Self::effective_limit).
    ///
    /// # Args
    /// - `rows`: The rows fetched from the database using [`fetch_limit`](Self::fetch_limit).
    pub fn into_response_parts<R>(&self, mut rows: Vec<R>) -> ResponseParts<R, T, Id>
    where
        R: PaginatedRow<Id, T>,
        T: Copy,
    {
        let limit = self.effective_limit() as usize;

        if rows.len() <= limit {
            return Ok((rows, None));
        }

        rows.truncate(limit);

        let last_row = rows
            .last()
            .expect("effective_limit() > 0 so rows is non-empty");

        let (col, dir) = self.order_by();
        let last_id = last_row.row_id()?;
        let last_val = last_row.ordering_column_value(col)?;

        let next_cursor =
            CursorPayload::with_filter_sig(col, dir, last_id, last_val, self.filter_sig());

        Ok((rows, Some(next_cursor)))
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::pagination::cursor::{CursorPayload, CursorValue};
    use crate::types::{DatabaseTable, DatabaseTableColumn};

    // --- Test types ----------------------------------------------------

    #[derive(Debug, Copy, Clone)]
    struct TestTable;
    impl DatabaseTable for TestTable {
        type Columns = TestTableColumn;
        const NAME: &'static str = "test_table";
        fn primary_key() -> Self::Columns {
            Self::Columns::Id
        }
    }

    /// Full table-column enum for the test table; implements
    /// [`DatabaseTableColumn`]. The `strum` derives supply the SQL column
    /// names required by the trait's `Into<&'static str>` / `VariantNames`
    /// bounds.
    #[derive(
        Debug,
        Default,
        Copy,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Serialize,
        Deserialize,
        strum::Display,
        strum::IntoStaticStr,
        strum::VariantNames,
        strum::EnumIter,
    )]
    #[strum(serialize_all = "snake_case")]
    enum TestTableColumn {
        #[default]
        Id,
        Name,
        #[strum(serialize = "iso2")]
        Iso2,
        Status,
        CreatedAt,
        OwnerId,
        Avatar,
    }

    impl DatabaseTableColumn for TestTableColumn {
        type Table = TestTable;
        fn table_primary_key() -> Self {
            Self::Table::primary_key()
        }
    }

    impl crate::types::ColumnTypeInfo for TestTableColumn {
        fn pg_type_info(&self) -> sqlx::postgres::PgTypeInfo {
            match self {
                Self::Id => <i32 as sqlx::Type<sqlx::Postgres>>::type_info(),
                Self::Name | Self::Iso2 => <String as sqlx::Type<sqlx::Postgres>>::type_info(),
                // A by-name type, like sqlx's derive emits for Postgres enums.
                Self::Status => sqlx::postgres::PgTypeInfo::with_name("test_status"),
                Self::CreatedAt => {
                    <chrono::DateTime<chrono::Utc> as sqlx::Type<sqlx::Postgres>>::type_info()
                }
                Self::OwnerId => <uuid::Uuid as sqlx::Type<sqlx::Postgres>>::type_info(),
                // `bytea` maps to no CursorValue variant: the unclassifiable case.
                Self::Avatar => <Vec<u8> as sqlx::Type<sqlx::Postgres>>::type_info(),
            }
        }
    }

    #[derive(Serialize)]
    struct TestFilters<'a> {
        region: Option<&'a str>,
    }

    // --- Helpers --------------------------------------------------------

    fn cursor_for(col: TestTableColumn, dir: OrderByDirection) -> String {
        CursorPayload::with_filter_sig(
            col,
            dir,
            42_i32,
            Some(CursorValue::Text("value".to_string())),
            None,
        )
        .encode()
        .unwrap()
    }

    fn cursor_with_filters(
        col: TestTableColumn,
        dir: OrderByDirection,
        filters: &TestFilters,
    ) -> String {
        CursorPayload::try_new(
            col,
            dir,
            42_i32,
            Some(CursorValue::Text("value".to_string())),
            Some(filters),
        )
        .unwrap()
        .encode()
        .unwrap()
    }

    /// Builds a projection that includes the named columns plus the PK.
    fn projection_with(
        cols: impl IntoIterator<Item = TestTableColumn>,
    ) -> ColumnSelection<TestTableColumn> {
        ColumnSelection::new(cols)
    }

    // --- with_validated_column_selection: pagination mechanics ----------
    // These tests exercise limit clamping / cursor reconciliation / filter
    // signature reconciliation. They go through the pre-validated constructor
    // because they're not testing the projection check.

    #[test]
    fn effective_limit_clamps_to_max() -> Result<(), CursorPaginationError> {
        let c = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            OrderBy::<TestTableColumn>::default(),
            10_000,
            None,
            None,
        )?;
        assert_eq!(c.effective_limit(), MAX_PAGE_SIZE);
        Ok(())
    }

    #[test]
    fn effective_limit_passes_through_normal_value() -> Result<(), CursorPaginationError> {
        let c = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            OrderBy::<TestTableColumn>::default(),
            5,
            None,
            None,
        )?;
        assert_eq!(c.effective_limit(), 5);
        Ok(())
    }

    #[test]
    fn fetch_limit_is_effective_plus_one() -> Result<(), CursorPaginationError> {
        let c = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            OrderBy::<TestTableColumn>::default(),
            20,
            None,
            None,
        )?;
        assert_eq!(c.fetch_limit(), 21);
        Ok(())
    }

    #[test]
    fn zero_limit_uses_default() -> Result<(), CursorPaginationError> {
        let c = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            OrderBy::<TestTableColumn>::default(),
            0,
            None,
            None,
        )?;
        assert_eq!(c.effective_limit(), DEFAULT_CURSOR_LIMIT);
        Ok(())
    }

    #[test]
    fn has_no_cursor_and_default_limit_with_default_ordering() -> Result<(), CursorPaginationError>
    {
        let p = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            OrderBy::new(TestTableColumn::Name, OrderByDirection::Asc),
            DEFAULT_CURSOR_LIMIT,
            None,
            None,
        )?;
        assert!(p.cursor().is_none());
        assert_eq!(p.effective_limit(), DEFAULT_CURSOR_LIMIT);
        assert_eq!(p.order_by(), (TestTableColumn::Name, OrderByDirection::Asc));
        Ok(())
    }

    #[test]
    fn accepts_matching_cursor() -> Result<(), CursorPaginationError> {
        let cursor = cursor_for(TestTableColumn::Iso2, OrderByDirection::Desc);
        let ordering = OrderBy::new(TestTableColumn::Iso2, OrderByDirection::Desc);

        let p = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            ordering,
            20,
            Some(cursor),
            None,
        )?;

        let payload = p.cursor().expect("cursor should be present");
        assert_eq!(payload.col, TestTableColumn::Iso2);
        assert_eq!(payload.dir, OrderByDirection::Desc);
        assert_eq!(payload.id, 42);
        Ok(())
    }

    #[test]
    fn reconcile_drops_cursor_when_col_mismatches() -> Result<(), CursorPaginationError> {
        let cursor = cursor_for(TestTableColumn::Iso2, OrderByDirection::Desc);
        let ordering = OrderBy::new(TestTableColumn::Name, OrderByDirection::Desc);
        let p = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            ordering,
            20,
            Some(cursor),
            None,
        )?;
        assert!(p.cursor().is_none());
        Ok(())
    }

    #[test]
    fn accepts_matching_filters() -> Result<(), CursorPaginationError> {
        let filters = TestFilters {
            region: Some("Europe"),
        };
        let cursor = cursor_with_filters(TestTableColumn::Name, OrderByDirection::Asc, &filters);
        let ordering = OrderBy::new(TestTableColumn::Name, OrderByDirection::Asc);
        let p = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection(
            ordering,
            20,
            Some(cursor),
            Some(&filters),
        )?;
        assert!(p.cursor().is_some());
        Ok(())
    }

    /// Reconciles a cursor carrying `val` for `col` against a listing sorted
    /// by that same column and direction, so only the value's type can differ.
    fn reconcile_value(
        col: TestTableColumn,
        val: CursorValue,
    ) -> Result<Option<CursorPayload<TestTableColumn, i32>>, CursorPaginationError> {
        let cursor =
            CursorPayload::with_filter_sig(col, OrderByDirection::Asc, 42_i32, Some(val), None)
                .encode()?;
        let p = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            OrderBy::new(col, OrderByDirection::Asc),
            20,
            Some(cursor),
            None,
        )?;
        Ok(p.cursor().cloned())
    }

    fn assert_value_type_mismatch(result: Result<impl std::fmt::Debug, CursorPaginationError>) {
        match result {
            Err(CursorPaginationError::CursorError(CursorError::ValueTypeMismatch { .. })) => {}
            other => panic!("expected CursorError::ValueTypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn rejects_enum_value_on_builtin_column() {
        assert_value_type_mismatch(reconcile_value(
            TestTableColumn::Name,
            CursorValue::Enum("active".to_string()),
        ));
    }

    #[test]
    fn rejects_scalar_value_on_by_name_column() {
        assert_value_type_mismatch(reconcile_value(
            TestTableColumn::Status,
            CursorValue::Text("active".to_string()),
        ));
    }

    #[test]
    fn rejects_text_value_on_timestamptz_column() {
        assert_value_type_mismatch(reconcile_value(
            TestTableColumn::CreatedAt,
            CursorValue::Text("2024-01-15T10:30:00Z".to_string()),
        ));
    }

    #[test]
    fn rejects_text_value_on_uuid_column() {
        assert_value_type_mismatch(reconcile_value(
            TestTableColumn::OwnerId,
            CursorValue::Text("018e3c8e-8b2a-7b4b-bb2a-1d2e3c4b5a6f".to_string()),
        ));
    }

    #[test]
    fn rejects_text_value_on_integer_column() {
        assert_value_type_mismatch(reconcile_value(
            TestTableColumn::Id,
            CursorValue::Text("42".to_string()),
        ));
    }

    #[test]
    fn rejects_timestamp_value_on_uuid_column() {
        assert_value_type_mismatch(reconcile_value(
            TestTableColumn::OwnerId,
            CursorValue::DateTimeUtc(chrono::Utc::now()),
        ));
    }

    #[test]
    fn mismatch_error_names_the_value_and_column_types() {
        match reconcile_value(
            TestTableColumn::CreatedAt,
            CursorValue::Text("nope".to_string()),
        ) {
            Err(CursorPaginationError::CursorError(CursorError::ValueTypeMismatch {
                value,
                column,
            })) => {
                assert_eq!(value, "text");
                assert_eq!(column, "TIMESTAMPTZ");
            }
            other => panic!("expected CursorError::ValueTypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn keeps_matching_scalar_values() -> Result<(), CursorPaginationError> {
        assert!(reconcile_value(TestTableColumn::Id, CursorValue::Int(42))?.is_some());
        assert!(
            reconcile_value(
                TestTableColumn::Name,
                CursorValue::Text("Dubai".to_string())
            )?
            .is_some()
        );
        assert!(
            reconcile_value(
                TestTableColumn::CreatedAt,
                CursorValue::DateTimeUtc(chrono::Utc::now())
            )?
            .is_some()
        );
        assert!(
            reconcile_value(
                TestTableColumn::OwnerId,
                CursorValue::Uuid(uuid::Uuid::nil())
            )?
            .is_some()
        );
        Ok(())
    }

    #[test]
    fn keeps_cursor_on_column_type_it_cannot_classify() -> Result<(), CursorPaginationError> {
        assert!(
            reconcile_value(
                TestTableColumn::Avatar,
                CursorValue::Text("anything".to_string())
            )?
            .is_some()
        );
        Ok(())
    }

    #[test]
    fn keeps_cursor_with_enum_value_on_by_name_column() -> Result<(), CursorPaginationError> {
        let cursor = CursorPayload::with_filter_sig(
            TestTableColumn::Status,
            OrderByDirection::Asc,
            42_i32,
            Some(CursorValue::Enum("active".to_string())),
            None,
        )
        .encode()?;
        let ordering = OrderBy::new(TestTableColumn::Status, OrderByDirection::Asc);
        let p = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            ordering,
            20,
            Some(cursor),
            None,
        )?;
        assert!(p.cursor().is_some());
        Ok(())
    }

    #[test]
    fn keeps_cursor_with_null_value_on_by_name_column() -> Result<(), CursorPaginationError> {
        let cursor = CursorPayload::with_filter_sig(
            TestTableColumn::Status,
            OrderByDirection::Asc,
            42_i32,
            None,
            None,
        )
        .encode()?;
        let ordering = OrderBy::new(TestTableColumn::Status, OrderByDirection::Asc);
        let p = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection::<()>(
            ordering,
            20,
            Some(cursor),
            None,
        )?;
        assert!(p.cursor().is_some());
        Ok(())
    }

    #[test]
    fn drops_cursor_when_filters_change() -> Result<(), CursorPaginationError> {
        let original = TestFilters {
            region: Some("Europe"),
        };
        let cursor = cursor_with_filters(TestTableColumn::Name, OrderByDirection::Asc, &original);
        let changed = TestFilters {
            region: Some("Asia"),
        };
        let ordering = OrderBy::new(TestTableColumn::Name, OrderByDirection::Asc);
        let p = CursorPagination::<TestTableColumn, i32>::with_validated_column_selection(
            ordering,
            20,
            Some(cursor),
            Some(&changed),
        )?;
        assert!(p.cursor().is_none());
        Ok(())
    }

    // --- try_new: projection-aware path -------------------------------

    #[test]
    fn try_new_accepts_when_sort_column_is_projected() -> Result<(), CursorPaginationError> {
        // Sort by Name, projection includes Name → should succeed.
        let projection = projection_with([TestTableColumn::Name]);
        let ordering = OrderBy::new(TestTableColumn::Name, OrderByDirection::Asc);

        let p = CursorPagination::<TestTableColumn, i32>::try_new::<()>(
            ordering,
            20,
            &projection,
            None,
            None,
        )?;
        assert_eq!(p.order_by(), (TestTableColumn::Name, OrderByDirection::Asc));
        assert!(p.cursor().is_none());
        Ok(())
    }

    #[test]
    fn try_new_succeeds_when_projection_only_has_pk() {
        // Projection given only the PK (auto-injected). Sort by Name:
        // Name is NOT projected → must return OrderingColumnNotProjected,
        // and the error must name the offending column.
        let projection = projection_with([]); // → just contains Id (auto-injected PK)
        let ordering = OrderBy::new(TestTableColumn::Name, OrderByDirection::Asc);

        let result = CursorPagination::<TestTableColumn, i32>::try_new::<()>(
            ordering,
            20,
            &projection,
            None,
            None,
        );
        match result {
            Err(CursorPaginationError::OrderingColumnNotProjected { column }) => {
                assert_eq!(column, "name");
            }
            other => panic!("expected OrderingColumnNotProjected, got {other:?}"),
        }
    }

    #[test]
    fn try_new_returns_err_when_sort_column_not_in_projection() {
        // Projection has Iso2 (and PK); sort is by Name → must fail and
        // include the offending column in the error.
        let projection = projection_with([TestTableColumn::Iso2]);
        let ordering = OrderBy::new(TestTableColumn::Name, OrderByDirection::Asc);

        let result = CursorPagination::<TestTableColumn, i32>::try_new::<()>(
            ordering,
            20,
            &projection,
            None,
            None,
        );
        match result {
            Err(CursorPaginationError::OrderingColumnNotProjected { column }) => {
                assert_eq!(column, "name");
            }
            other => panic!("expected OrderingColumnNotProjected, got {other:?}"),
        }
    }

    #[test]
    fn try_new_accepts_matching_cursor_with_valid_projection() -> Result<(), CursorPaginationError>
    {
        // Full path: try_new accepts a cursor through the projection-aware
        // constructor when the sort column is projected. This proves the
        // projection check doesn't break the existing cursor reconciliation.
        let projection = projection_with([TestTableColumn::Iso2]);
        let cursor = cursor_for(TestTableColumn::Iso2, OrderByDirection::Desc);
        let ordering = OrderBy::new(TestTableColumn::Iso2, OrderByDirection::Desc);

        let p = CursorPagination::<TestTableColumn, i32>::try_new::<()>(
            ordering,
            20,
            &projection,
            Some(cursor),
            None,
        )?;

        let payload = p.cursor().expect("cursor should be present");
        assert_eq!(payload.col, TestTableColumn::Iso2);
        assert_eq!(payload.dir, OrderByDirection::Desc);
        assert_eq!(payload.id, 42);
        Ok(())
    }
}
