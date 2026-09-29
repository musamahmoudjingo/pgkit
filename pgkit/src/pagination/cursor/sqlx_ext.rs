//! Cursor pagination extension for raw [`sqlx::QueryBuilder`].
//!
//! For the [`SelectBuilder`] integration, see
//! [`crate::query_builder::SelectBuilder::cursor_paginate`].

use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::push_keyset_predicate::push_keyset_predicate;
use super::push_order_and_limit::push_order_and_limit;
use super::{CursorPagination, CursorValue};
use crate::types::{ColumnRef, ColumnTypeInfo, DatabaseTableColumn};

/// Extension trait that adds cursor pagination to [`sqlx::QueryBuilder`],
/// the hand-rolled counterpart to
/// [`crate::query_builder::SelectBuilder::cursor_paginate`].
pub trait SqlxCursorPaginationExt<'a>: Sized {
    /// Apply cursor pagination: appends the keyset predicate (if a cursor
    /// is present), then ` ORDER BY <col>, <pk>` with matching
    /// `NULLS FIRST/LAST`, then ` LIMIT $n`.
    ///
    /// The predicate is joined with `AND` when the query already has a
    /// `WHERE` and opens one when it does not.
    fn cursor_paginate<T, Id>(&mut self, pagination: &CursorPagination<T, Id>) -> &mut Self
    where
        T: DatabaseTableColumn + ColumnTypeInfo + Copy,
        Id: Encode<'a, Postgres> + Type<Postgres> + Send + Clone + 'a;
}

impl<'a> SqlxCursorPaginationExt<'a> for QueryBuilder<Postgres> {
    fn cursor_paginate<T, Id>(&mut self, pagination: &CursorPagination<T, Id>) -> &mut Self
    where
        T: DatabaseTableColumn + ColumnTypeInfo + Copy,
        Id: Encode<'a, Postgres> + Type<Postgres> + Send + Clone + 'a,
    {
        let (col, _) = pagination.order_by();
        let col_ref = ColumnRef::from(col);
        let id_col_ref = ColumnRef::from(T::table_primary_key());

        if let Some(cursor) = pagination.cursor() {
            // Cast target for an enum sort value: server-derived, never from
            // the cursor. `None` for scalar columns.
            let cast = match &cursor.val {
                Some(CursorValue::Enum(_)) => Some(col.pg_type_info()),
                _ => None,
            };

            let has_where = self.sql().as_ref().to_ascii_uppercase().contains(" WHERE ");
            self.push(if has_where { " AND " } else { " WHERE " });
            push_keyset_predicate(
                self,
                &col_ref,
                &id_col_ref,
                cursor.dir,
                cursor.val.clone(),
                cursor.id.clone(),
                cast,
            );
        }

        push_order_and_limit(self, pagination, &id_col_ref);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::super::CursorValue;
    use super::*;
    use crate::ordering::{OrderBy, OrderByDirection};
    use crate::pagination::cursor::CursorPayload;
    use crate::test_support::CountriesColumn;

    #[test]
    fn query_builder_cursor_paginate_no_cursor() {
        let pagination =
            CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
                OrderBy::new(CountriesColumn::Name, OrderByDirection::Asc),
                20,
                None,
                None,
            )
            .unwrap();

        let mut qb = sqlx::QueryBuilder::<Postgres>::new(
            "SELECT * FROM geo_countries WHERE deleted_at IS NULL",
        );
        qb.cursor_paginate(&pagination);

        assert_eq!(
            qb.sql(),
            "SELECT * FROM geo_countries WHERE deleted_at IS NULL ORDER BY \
             geo_countries.name ASC NULLS LAST, geo_countries.id ASC NULLS LAST LIMIT $1"
        );
    }

    #[test]
    fn query_builder_cursor_paginate_with_cursor() {
        let cursor = CursorPayload::with_filter_sig(
            CountriesColumn::Name,
            OrderByDirection::Asc,
            42_i32,
            Some(CursorValue::Text("alice".to_string())),
            None,
        )
        .encode()
        .unwrap();
        let pagination =
            CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
                OrderBy::new(CountriesColumn::Name, OrderByDirection::Asc),
                20,
                Some(cursor),
                None,
            )
            .unwrap();

        let mut qb = sqlx::QueryBuilder::<Postgres>::new(
            "SELECT * FROM geo_countries WHERE deleted_at IS NULL",
        );
        qb.cursor_paginate(&pagination);

        assert_eq!(
            qb.sql(),
            "SELECT * FROM geo_countries WHERE deleted_at IS NULL AND \
             (geo_countries.name > $1 OR (geo_countries.name = $2 AND geo_countries.id > $3) OR geo_countries.name IS NULL) \
             ORDER BY geo_countries.name ASC NULLS LAST, geo_countries.id ASC NULLS LAST LIMIT $4"
        );
    }

    /// A query with no `WHERE` must get one, not a dangling ` AND `.
    #[test]
    fn a_query_without_a_where_gets_one() {
        let cursor = CursorPayload::with_filter_sig(
            CountriesColumn::Name,
            OrderByDirection::Asc,
            7_i32,
            Some(CursorValue::Text("Kenya".to_string())),
            None,
        )
        .encode()
        .unwrap();
        let pagination =
            CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
                OrderBy::new(CountriesColumn::Name, OrderByDirection::Asc),
                20,
                Some(cursor),
                None,
            )
            .unwrap();

        let mut qb = sqlx::QueryBuilder::<Postgres>::new("SELECT * FROM geo_countries");
        qb.cursor_paginate(&pagination);
        let sql = qb.sql().as_ref().to_string();

        assert!(
            sql.contains("geo_countries WHERE ("),
            "the predicate must open the clause: {sql}",
        );
        assert!(
            !sql.contains("geo_countries AND"),
            "must not join onto a clause that is not there: {sql}",
        );
    }

    /// A query that already has a `WHERE` is still joined with `AND`.
    #[test]
    fn a_query_with_a_where_is_joined_with_and() {
        let cursor = CursorPayload::with_filter_sig(
            CountriesColumn::Name,
            OrderByDirection::Asc,
            7_i32,
            Some(CursorValue::Text("Kenya".to_string())),
            None,
        )
        .encode()
        .unwrap();
        let pagination =
            CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
                OrderBy::new(CountriesColumn::Name, OrderByDirection::Asc),
                20,
                Some(cursor),
                None,
            )
            .unwrap();

        let mut qb = sqlx::QueryBuilder::<Postgres>::new(
            "SELECT * FROM geo_countries WHERE deleted_at IS NULL",
        );
        qb.cursor_paginate(&pagination);
        let sql = qb.sql().as_ref().to_string();

        assert!(sql.contains("IS NULL AND ("), "{sql}");
    }
}
