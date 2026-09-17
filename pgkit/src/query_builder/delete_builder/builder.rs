use sqlx::{Postgres, QueryBuilder};

use crate::query_builder::exec::{BuildQuery, BuildQueryAs, BuildQueryScalar};
use crate::query_builder::insert_builder::Returning;
use crate::query_builder::ops::HasWhere;
use crate::query_builder::where_builder::WhereBuilder;
use crate::types::{ColumnRef, DatabaseTableColumn, TableRef};

/// Fluent builder for PostgreSQL `DELETE` statements.
///
/// WHERE-clause methods (`where_eq`, `where_in`, `group`, etc.) come from the
/// [`WhereOps`](crate::query_builder::WhereOps) blanket impl; bring it into
/// scope to use them.
///
/// ```no_run
/// # #[derive(sqlx::FromRow, pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, iso2: String, name: String, deleted_at: Option<chrono::DateTime<chrono::Utc>> }
/// # async fn demo(pool: sqlx::PgPool, id: i32) -> Result<(), Box<dyn std::error::Error>> {
/// use pgkit::query_builder::{Query, WhereOps};
///
/// let rows = Query::delete().from(CountriesTable)
///     .where_eq(CountriesColumn::Id, id)
///     .returning_all()
///     .build_query_as::<CountriesRow>()
///     .fetch_all(&pool).await?;
/// # Ok(()) }
/// ```
pub struct DeleteBuilder<'a> {
    table: Option<TableRef>,
    using: Vec<TableRef>,
    where_clause: WhereBuilder<'a>,
    returning: Returning,
    allow_unfiltered: bool,
}

impl<'a> DeleteBuilder<'a> {
    /// Empty builder. Prefer [`Query::delete()`](crate::query_builder::Query::delete).
    pub fn new() -> Self {
        Self {
            table: None,
            using: Vec::new(),
            where_clause: WhereBuilder::new(),
            returning: Returning::None,
            allow_unfiltered: false,
        }
    }

    /// Explicitly opt into a whole-table `DELETE` with no WHERE clause.
    /// Without this, [`build`](Self::build) panics on an empty predicate: a
    /// `where_eq` dropped in a refactor (or a conditional chain that resolved
    /// to nothing) must not silently wipe the table.
    pub fn all(mut self) -> Self {
        self.allow_unfiltered = true;
        self
    }

    /// `DELETE FROM <table>`.
    pub fn from<T: Into<TableRef>>(mut self, table: T) -> Self {
        self.table = Some(table.into());
        self
    }

    /// `USING <table>`: auxiliary table(s) for the WHERE predicate (Postgres
    /// extension). Repeated calls accumulate.
    pub fn using<T: Into<TableRef>>(mut self, table: T) -> Self {
        self.using.push(table.into());
        self
    }

    // ––– RETURNING –––

    /// `RETURNING *`.
    pub fn returning_all(mut self) -> Self {
        self.returning = Returning::All;
        self
    }

    /// `RETURNING col1, col2, …`.
    pub fn returning<C, I>(mut self, columns: I) -> Self
    where
        C: Into<ColumnRef>,
        I: IntoIterator<Item = C>,
    {
        self.returning = Returning::Columns(columns.into_iter().map(Into::into).collect());
        self
    }

    /// `RETURNING <every column declared on T>`.
    pub fn returning_all_of<T: DatabaseTableColumn>(mut self) -> Self {
        let cols = T::iter().map(ColumnRef::from).collect();
        self.returning = Returning::Columns(cols);
        self
    }

    /// `RETURNING <raw-sql>`: escape hatch.
    pub fn returning_raw(mut self, sql: impl Into<String>) -> Self {
        self.returning = Returning::Raw(sql.into());
        self
    }

    // ––– Terminal: render –––

    /// Render into a fresh [`sqlx::QueryBuilder`]. Chain sqlx terminals
    /// (`.build_query()`, `.build_query_as()`, etc.) on the result.
    ///
    /// # Panics
    ///
    /// - If [`from`](Self::from) was never called.
    /// - If the WHERE clause is empty and [`all`](Self::all) was not called.
    pub fn build(self) -> QueryBuilder<Postgres> {
        let table = self
            .table
            .expect("DeleteBuilder: missing `.from(table)` call");

        if self.where_clause.is_unfiltered() && !self.allow_unfiltered {
            panic!(
                "DeleteBuilder: DELETE FROM {table} has no effective WHERE clause; every \
                 condition matches every row (an empty `NOT IN` / `<> ALL` list renders as TRUE), \
                 so this deletes the whole table. Call `.all()` to opt in, or skip the query when \
                 the list is empty."
            );
        }

        let mut query = sqlx::QueryBuilder::new(format!("DELETE FROM {table}"));

        if !self.using.is_empty() {
            query.push(" USING ");
            for (i, t) in self.using.iter().enumerate() {
                if i > 0 {
                    query.push(", ");
                }
                query.push(t);
            }
        }

        self.where_clause.apply_to(&mut query);
        self.returning.apply_to(&mut query);

        query
    }

    /// Shortcut for `.build().build()` wrapped in a [`BuildQuery`] so
    /// callers can chain `.execute(...)` / `.fetch_one(...)` directly.
    pub fn build_query(self) -> BuildQuery {
        BuildQuery::new(self.build())
    }

    /// Shortcut for `.build().build_query_as::<T>()` wrapped in a
    /// [`BuildQueryAs`] so callers can chain `.fetch_one(...)` directly.
    pub fn build_query_as<T>(self) -> BuildQueryAs<T> {
        BuildQueryAs::new(self.build())
    }

    /// Shortcut for `.build().build_query_scalar::<T>()` wrapped in a
    /// [`BuildQueryScalar`] so callers can chain `.fetch_one(...)` directly.
    pub fn build_query_scalar<T>(self) -> BuildQueryScalar<T> {
        BuildQueryScalar::new(self.build())
    }
}

impl<'a> Default for DeleteBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> HasWhere<'a> for DeleteBuilder<'a> {
    fn where_mut(&mut self) -> &mut WhereBuilder<'a> {
        &mut self.where_clause
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_builder::ops::WhereOps;
    use crate::test_support::{FakeColumn, FakeTable};

    #[test]
    fn delete_with_where_eq() {
        let q = DeleteBuilder::new().from("t").where_eq("id", 1_i32).build();

        assert_eq!(q.sql(), "DELETE FROM t WHERE id = $1");
    }

    #[test]
    fn delete_with_multiple_predicates() {
        let q = DeleteBuilder::new()
            .from("t")
            .where_eq("id", 1_i32)
            .where_null("deleted_at")
            .build();

        assert_eq!(
            q.sql(),
            "DELETE FROM t WHERE id = $1 AND deleted_at IS NULL"
        );
    }

    #[test]
    #[should_panic(expected = "no effective WHERE clause")]
    fn delete_without_where_clause_panics() {
        let _ = DeleteBuilder::new().from("t").build();
    }

    /// An empty `NOT IN` renders as a constant `TRUE`, so the old `is_empty()`
    /// guard passed it and the DELETE emptied the table.
    #[test]
    #[should_panic(expected = "no effective WHERE clause")]
    fn delete_filtered_only_by_an_empty_not_in_panics() {
        let _ = DeleteBuilder::new()
            .from("t")
            .where_not_in("id", Vec::<i32>::new())
            .build();
    }

    #[test]
    fn delete_all_requires_explicit_opt_in() {
        let q = DeleteBuilder::new().from("t").all().build();
        assert_eq!(q.sql(), "DELETE FROM t");
    }

    #[test]
    fn delete_returning_all() {
        let q = DeleteBuilder::new()
            .from("t")
            .where_eq("id", 1_i32)
            .returning_all()
            .build();

        assert_eq!(q.sql(), "DELETE FROM t WHERE id = $1 RETURNING *");
    }

    #[test]
    fn delete_returning_columns() {
        let q = DeleteBuilder::new()
            .from("t")
            .where_eq("id", 1_i32)
            .returning(["id", "name"])
            .build();

        assert_eq!(q.sql(), "DELETE FROM t WHERE id = $1 RETURNING id, name");
    }

    #[test]
    fn delete_using_auxiliary_table() {
        let q = DeleteBuilder::new()
            .from("t")
            .using("u")
            .where_raw("t.uid = u.id")
            .where_eq("u.active", false)
            .build();

        assert_eq!(
            q.sql(),
            "DELETE FROM t USING u WHERE t.uid = u.id AND u.active = $1"
        );
    }

    #[test]
    fn delete_with_in_clause() {
        let q = DeleteBuilder::new()
            .from("t")
            .where_in("id", vec![1_i32, 2, 3])
            .build();

        assert_eq!(q.sql(), "DELETE FROM t WHERE id IN ($1, $2, $3)");
    }

    #[test]
    fn delete_with_when_conditional() {
        let include_soft = true;
        let q = DeleteBuilder::new()
            .from("t")
            .where_eq("id", 1_i32)
            .when(!include_soft, |b| b.where_null("deleted_at"))
            .build();

        // include_soft=true → where_null skipped
        assert_eq!(q.sql(), "DELETE FROM t WHERE id = $1");
    }

    #[test]
    #[should_panic(expected = "missing `.from(table)`")]
    fn build_without_from_panics() {
        let _ = DeleteBuilder::new().where_eq("id", 1_i32).build();
    }

    #[test]
    fn typed_columns_in_returning_render_unqualified() {
        let q = DeleteBuilder::new()
            .from(FakeTable)
            .where_eq("id", 1_i32)
            .returning([FakeColumn::Id, FakeColumn::Name])
            .build();
        assert_eq!(
            q.sql(),
            "DELETE FROM fake_table WHERE id = $1 RETURNING id, name"
        );
    }
}
