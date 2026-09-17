use sqlx::{Encode, Postgres, QueryBuilder, Type};

use crate::query_builder::exec::{BuildQuery, BuildQueryAs, BuildQueryScalar};
use crate::query_builder::insert_builder::Returning;
use crate::query_builder::ops::HasWhere;
use crate::query_builder::where_builder::WhereBuilder;
use crate::types::{ColumnRef, DatabaseTableColumn, TableRef};

/// Boxed closure that pushes a single value or expression into the SET list.
type Setter<'a> = Box<dyn FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a>;

/// A single `col = …` assignment inside the `SET` clause.
struct SetItem<'a> {
    column: ColumnRef,
    rhs: Setter<'a>,
}

/// Fluent builder for PostgreSQL `UPDATE` statements.
///
/// PATCH-style partial updates are first-class via [`set_if_some`](Self::set_if_some)
/// and [`set_nullable`](Self::set_nullable).
///
/// WHERE-clause methods come from [`WhereOps`](crate::query_builder::WhereOps).
///
/// ```no_run
/// # #[derive(sqlx::FromRow, pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, iso2: String, name: String, deleted_at: Option<chrono::DateTime<chrono::Utc>> }
/// # struct Payload { name: Option<String> }
/// # async fn demo(pool: sqlx::PgPool, id: i32, payload: Payload) -> Result<(), Box<dyn std::error::Error>> {
/// use pgkit::query_builder::{Query, WhereOps};
///
/// let row = Query::update().table("geo_countries")
///     .set("iso2", "UG")
///     .set_if_some("name", payload.name.as_ref())
///     .set_raw("updated_at", |q| { q.push("NOW()"); })
///     .where_eq("id", id)
///     .where_null("deleted_at")
///     .returning_all()
///     .build_query_as::<CountriesRow>()
///     .fetch_one(&pool).await?;
/// # Ok(()) }
/// ```
pub struct UpdateBuilder<'a> {
    table: Option<TableRef>,
    sets: Vec<SetItem<'a>>,
    from: Vec<TableRef>,
    where_clause: WhereBuilder<'a>,
    returning: Returning,
    allow_unfiltered: bool,
}

impl<'a> UpdateBuilder<'a> {
    /// Empty builder. Prefer [`Query::update()`](crate::query_builder::Query::update).
    pub fn new() -> Self {
        Self {
            table: None,
            sets: Vec::new(),
            from: Vec::new(),
            where_clause: WhereBuilder::new(),
            returning: Returning::None,
            allow_unfiltered: false,
        }
    }

    /// Explicitly opt into a whole-table `UPDATE` with no WHERE clause.
    /// Without this, [`build`](Self::build) panics on an empty predicate: a
    /// `where_eq` dropped in a refactor (or a conditional chain that resolved
    /// to nothing) must not silently rewrite every row.
    pub fn all(mut self) -> Self {
        self.allow_unfiltered = true;
        self
    }

    /// `UPDATE <table>`.
    pub fn table<T: Into<TableRef>>(mut self, table: T) -> Self {
        self.table = Some(table.into());
        self
    }

    /// `FROM <table>`: auxiliary table(s) for joins inside the WHERE
    /// predicate (Postgres extension). Repeated calls accumulate.
    pub fn from<T: Into<TableRef>>(mut self, table: T) -> Self {
        self.from.push(table.into());
        self
    }

    // ––– SET clauses –––

    /// `SET col = $N`: bind `value` as a parameter.
    pub fn set<C, T>(mut self, column: C, value: T) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.sets.push(SetItem {
            column: column.into(),
            rhs: Box::new(move |q| {
                q.push_bind(value);
            }),
        });
        self
    }

    /// Only emit `SET col = …` if `value` is `Some`. Designed for PATCH
    /// payloads where unset fields are `Option::None`.
    pub fn set_if_some<C, T>(self, column: C, value: Option<T>) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        match value {
            Some(v) => self.set(column, v),
            None => self,
        }
    }

    /// `SET col = <raw expression>`: closure pushes its own SQL (and binds).
    /// Use for things like `NOW()`, `col + 1`, function calls, casts.
    pub fn set_raw<C, F>(mut self, column: C, expr: F) -> Self
    where
        C: Into<ColumnRef>,
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.sets.push(SetItem {
            column: column.into(),
            rhs: Box::new(expr),
        });
        self
    }

    /// PATCH-semantics for nullable fields, expressed as
    /// `Option<Option<T>>`:
    ///
    /// - `None`            → field absent, no SET emitted.
    /// - `Some(None)`      → field explicitly null, emits `SET col = NULL`.
    /// - `Some(Some(v))`   → emits `SET col = $N` bound to `v`.
    pub fn set_nullable<C, T>(self, column: C, value: Option<Option<T>>) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        match value {
            None => self,
            Some(None) => self.set_raw(column, |q| {
                q.push("NULL");
            }),
            Some(Some(v)) => self.set(column, v),
        }
    }

    /// True when nothing has been pushed into `SET …`. Use for the
    /// "empty update payload" early-return pattern.
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
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

    /// `RETURNING <raw-sql>`.
    pub fn returning_raw(mut self, sql: impl Into<String>) -> Self {
        self.returning = Returning::Raw(sql.into());
        self
    }

    // ––– Terminal: render –––

    /// Render into a fresh [`sqlx::QueryBuilder`]. Chain sqlx terminals on
    /// the result.
    ///
    /// # Panics
    ///
    /// - If [`table`](Self::table) was never called.
    /// - If no SET assignments were added.
    /// - If the WHERE clause is empty and [`all`](Self::all) was not called.
    pub fn build(self) -> QueryBuilder<Postgres> {
        let table = self
            .table
            .expect("UpdateBuilder: missing `.table(table)` call");

        if self.where_clause.is_unfiltered() && !self.allow_unfiltered {
            panic!(
                "UpdateBuilder: UPDATE {table} has no effective WHERE clause; every condition \
                 matches every row (an empty `NOT IN` / `<> ALL` list renders as TRUE), so this \
                 rewrites the whole table. Call `.all()` to opt in, or skip the query when the \
                 list is empty."
            );
        }

        if self.sets.is_empty() {
            panic!(
                "UpdateBuilder: no SET assignments; call `.set(...)` / `.set_if_some(...)` / `.set_raw(...)`",
            );
        }

        let mut query = sqlx::QueryBuilder::new(format!("UPDATE {table} SET "));

        for (i, item) in self.sets.into_iter().enumerate() {
            if i > 0 {
                query.push(", ");
            }
            query.push(item.column.unqualified());
            query.push(" = ");
            (item.rhs)(&mut query);
        }

        if !self.from.is_empty() {
            query.push(" FROM ");
            for (i, t) in self.from.iter().enumerate() {
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

impl<'a> Default for UpdateBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> HasWhere<'a> for UpdateBuilder<'a> {
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
    fn simple_update_with_where() {
        let q = UpdateBuilder::new()
            .table("t")
            .set("a", 1_i32)
            .where_eq("id", 99_i32)
            .build();

        assert_eq!(q.sql(), "UPDATE t SET a = $1 WHERE id = $2");
    }

    #[test]
    fn multiple_set_clauses() {
        let q = UpdateBuilder::new()
            .table("t")
            .set("a", 1_i32)
            .set("b", "x")
            .set("c", true)
            .where_eq("id", 99_i32)
            .build();

        assert_eq!(q.sql(), "UPDATE t SET a = $1, b = $2, c = $3 WHERE id = $4");
    }

    #[test]
    fn set_if_some_skips_none() {
        let q = UpdateBuilder::new()
            .table("t")
            .set("a", 1_i32)
            .set_if_some("b", None::<&str>)
            .set("c", true)
            .where_eq("id", 99_i32)
            .build();

        assert_eq!(q.sql(), "UPDATE t SET a = $1, c = $2 WHERE id = $3");
    }

    #[test]
    fn set_if_some_includes_some() {
        let name = Some("Uganda");
        let q = UpdateBuilder::new()
            .table("t")
            .set_if_some("name", name)
            .where_eq("id", 99_i32)
            .build();

        assert_eq!(q.sql(), "UPDATE t SET name = $1 WHERE id = $2");
    }

    #[test]
    fn set_raw_uses_expression() {
        let q = UpdateBuilder::new()
            .table("t")
            .set("a", 1_i32)
            .set_raw("updated_at", |q| {
                q.push("NOW()");
            })
            .where_eq("id", 99_i32)
            .build();

        assert_eq!(
            q.sql(),
            "UPDATE t SET a = $1, updated_at = NOW() WHERE id = $2"
        );
    }

    #[test]
    fn update_with_returning_all() {
        let q = UpdateBuilder::new()
            .table("t")
            .set("a", 1_i32)
            .where_eq("id", 99_i32)
            .returning_all()
            .build();

        assert_eq!(q.sql(), "UPDATE t SET a = $1 WHERE id = $2 RETURNING *");
    }

    #[test]
    fn update_with_returning_columns() {
        let q = UpdateBuilder::new()
            .table("t")
            .set("a", 1_i32)
            .where_eq("id", 99_i32)
            .returning(["id", "a", "updated_at"])
            .build();

        assert_eq!(
            q.sql(),
            "UPDATE t SET a = $1 WHERE id = $2 RETURNING id, a, updated_at"
        );
    }

    #[test]
    fn soft_delete_pattern() {
        // Reproduce the canonical soft-delete pattern from the geo repos.
        let q = UpdateBuilder::new()
            .table("geo_countries")
            .set_raw("deleted_at", |q| {
                q.push("NOW()");
            })
            .where_eq("id", 1_i32)
            .where_null("deleted_at")
            .build();

        assert_eq!(
            q.sql(),
            "UPDATE geo_countries SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL"
        );
    }

    #[test]
    fn update_from_auxiliary_table() {
        let q = UpdateBuilder::new()
            .table("t")
            .set_raw("v", |q| {
                q.push("u.v");
            })
            .from("u")
            .where_raw("t.uid = u.id")
            .build();

        assert_eq!(q.sql(), "UPDATE t SET v = u.v FROM u WHERE t.uid = u.id");
    }

    #[test]
    fn is_empty_tracks_set_count() {
        let b: UpdateBuilder<'_> = UpdateBuilder::new().table("t");
        assert!(b.is_empty());

        let b = b.set("a", 1_i32);
        assert!(!b.is_empty());
    }

    #[test]
    #[should_panic(expected = "missing `.table(table)`")]
    fn build_without_table_panics() {
        let _ = UpdateBuilder::new().set("a", 1_i32).build();
    }

    #[test]
    #[should_panic(expected = "no SET assignments")]
    fn build_without_sets_panics() {
        let _ = UpdateBuilder::new()
            .table("t")
            .where_eq("id", 1_i32)
            .build();
    }

    #[test]
    #[should_panic(expected = "no effective WHERE clause")]
    fn update_without_where_clause_panics() {
        let _ = UpdateBuilder::new().table("t").set("name", "x").build();
    }

    /// The guard used to test `is_empty()`, which an empty `NOT IN` satisfies;
    /// it renders as a constant `TRUE`, so the UPDATE silently rewrote the table.
    #[test]
    #[should_panic(expected = "no effective WHERE clause")]
    fn update_filtered_only_by_an_empty_not_in_panics() {
        let _ = UpdateBuilder::new()
            .table("t")
            .set("name", "x")
            .where_not_in("id", Vec::<i32>::new())
            .build();
    }

    #[test]
    fn update_with_a_real_condition_beside_an_empty_not_in_is_allowed() {
        let q = UpdateBuilder::new()
            .table("t")
            .set("name", "x")
            .where_eq("id", 1_i32)
            .where_not_in("status", Vec::<i32>::new())
            .build();
        assert!(q.sql().as_ref().contains("WHERE"));
    }

    #[test]
    fn update_all_requires_explicit_opt_in() {
        let q = UpdateBuilder::new()
            .table("t")
            .set("name", "x")
            .all()
            .build();
        assert_eq!(q.sql(), "UPDATE t SET name = $1");
    }

    #[test]
    fn typed_columns_in_set_render_unqualified() {
        let q = UpdateBuilder::new()
            .table(FakeTable)
            .set(FakeColumn::Id, 1_i32)
            .set(FakeColumn::Name, "x")
            .where_eq("id", 99_i32)
            .build();
        assert_eq!(
            q.sql(),
            "UPDATE fake_table SET id = $1, name = $2 WHERE id = $3"
        );
    }

    #[test]
    fn typed_columns_with_table_alias_still_render_unqualified_in_set() {
        let q = UpdateBuilder::new()
            .table(FakeTable)
            .set(("ft", FakeColumn::Name), "x")
            .where_eq("id", 99_i32)
            .build();
        assert_eq!(q.sql(), "UPDATE fake_table SET name = $1 WHERE id = $2");
    }

    #[test]
    fn typed_columns_in_returning_render_unqualified() {
        let q = UpdateBuilder::new()
            .table(FakeTable)
            .set(FakeColumn::Name, "x")
            .where_eq("id", 99_i32)
            .returning([FakeColumn::Id, FakeColumn::Name])
            .build();
        assert_eq!(
            q.sql(),
            "UPDATE fake_table SET name = $1 WHERE id = $2 RETURNING id, name"
        );
    }

    mod set_nullable {
        use super::*;

        #[test]
        fn absent_skips_field() {
            let value: Option<Option<String>> = None;
            let q = UpdateBuilder::new()
                .table("t")
                .set("keep", 1_i32)
                .set_nullable("notes", value)
                .where_eq("id", 1_i32)
                .build();

            assert_eq!(q.sql(), "UPDATE t SET keep = $1 WHERE id = $2");
        }

        #[test]
        fn explicit_null_emits_null() {
            let value: Option<Option<String>> = Some(None);
            let q = UpdateBuilder::new()
                .table("t")
                .set_nullable("notes", value)
                .where_eq("id", 1_i32)
                .build();

            assert_eq!(q.sql(), "UPDATE t SET notes = NULL WHERE id = $1");
        }

        #[test]
        fn value_binds_value() {
            let value: Option<Option<String>> = Some(Some("hello".to_string()));
            let q = UpdateBuilder::new()
                .table("t")
                .set_nullable("notes", value)
                .where_eq("id", 1_i32)
                .build();

            assert_eq!(q.sql(), "UPDATE t SET notes = $1 WHERE id = $2");
        }
    }
}
