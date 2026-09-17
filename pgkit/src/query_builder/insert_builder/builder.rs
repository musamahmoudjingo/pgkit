use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::body::InsertBody;
use super::on_conflict::{ConflictTarget, OnConflict, OnConflictClause};
use super::returning::Returning;
use super::row::{Binder, InsertRow};
use crate::query_builder::exec::{BuildQuery, BuildQueryAs, BuildQueryScalar};
use crate::types::{ColumnRef, DatabaseTableColumn, TableRef};

/// Fluent builder for PostgreSQL `INSERT` statements.
///
/// Three input shapes, mutually exclusive within a single builder:
///
/// 1. **Paired single row**: `.value(col, val).value(col, val)` …
/// 2. **Multi-row VALUES**: `.columns([c1, c2]).row(|r| r.bind(v1).bind(v2)).row(...)`
/// 3. **INSERT … SELECT**: `.columns([c1, c2]).with_select(|q| q.push("SELECT …"))`
///
/// Plus `ON CONFLICT … DO NOTHING / DO UPDATE` and `RETURNING …` clauses.
///
/// ```no_run
/// # use pgkit::query_builder::Query;
/// # #[derive(sqlx::FromRow, pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, iso2: String, name: String, deleted_at: Option<chrono::DateTime<chrono::Utc>> }
/// # use CountriesColumn::*;
/// # struct Payload { iso2: String, name: String }
/// # async fn demo(pool: sqlx::PgPool, payload: Payload) -> Result<(), Box<dyn std::error::Error>> {
/// let row = Query::insert().into(CountriesTable)
///     .value(Iso2, payload.iso2)
///     .value(Name, payload.name)
///     .returning_all()
///     .build_query_as::<CountriesRow>()
///     .fetch_one(&pool).await?;
/// # Ok(()) }
/// ```
pub struct InsertBuilder<'a> {
    table: Option<TableRef>,
    body: InsertBody<'a>,
    pub(crate) on_conflict: Option<OnConflictClause<'a>>,
    returning: Returning,
}

impl<'a> InsertBuilder<'a> {
    /// Empty builder. Prefer [`Query::insert()`](crate::query_builder::Query::insert).
    pub fn new() -> Self {
        Self {
            table: None,
            body: InsertBody::Empty,
            on_conflict: None,
            returning: Returning::None,
        }
    }

    // ––– Target table –––

    /// `INSERT INTO <table>`.
    pub fn into<T: Into<TableRef>>(mut self, table: T) -> Self {
        self.table = Some(table.into());
        self
    }

    // ––– Single-row paired form –––

    /// Add one `(column, value)` pair. Repeated calls accumulate.
    ///
    /// Cannot be mixed with [`columns`](Self::columns)/[`row`](Self::row).
    pub fn value<C, T>(mut self, column: C, value: T) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let column = column.into();
        let bind: Binder<'a> = Box::new(move |q| {
            q.push_bind(value);
        });
        match &mut self.body {
            body @ InsertBody::Empty => {
                *body = InsertBody::Paired(vec![(column, bind)]);
            }
            InsertBody::Paired(items) => {
                items.push((column, bind));
            }
            _ => panic!(
                "InsertBuilder: `.value()` cannot be mixed with `.columns()` / `.with_select()`",
            ),
        }
        self
    }

    /// `.value(col, v)` only if `value` is `Some`, convenient for partial
    /// inserts where some optional fields may be absent.
    pub fn value_if_some<C, T>(self, column: C, value: Option<T>) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        match value {
            Some(v) => self.value(column, v),
            None => self,
        }
    }

    /// Add one `(column, <raw expression>)` pair. Closure pushes its own SQL
    /// (and binds, if any), useful for `DEFAULT`, `NOW()`, function calls.
    pub fn value_raw<C, F>(mut self, column: C, expr: F) -> Self
    where
        C: Into<ColumnRef>,
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        let column = column.into();
        let bind: Binder<'a> = Box::new(expr);
        match &mut self.body {
            body @ InsertBody::Empty => {
                *body = InsertBody::Paired(vec![(column, bind)]);
            }
            InsertBody::Paired(items) => {
                items.push((column, bind));
            }
            _ => panic!(
                "InsertBuilder: `.value_raw()` cannot be mixed with `.columns()` / `.with_select()`",
            ),
        }
        self
    }

    // ––– Bulk multi-row form –––

    /// Declare the columns for a multi-row or `INSERT … SELECT` form. Follow
    /// with one or more [`row`](Self::row) calls, or with
    /// [`with_select`](Self::with_select).
    ///
    /// Cannot be mixed with [`value`](Self::value).
    pub fn columns<C, I>(mut self, columns: I) -> Self
    where
        C: Into<ColumnRef>,
        I: IntoIterator<Item = C>,
    {
        let cols: Vec<ColumnRef> = columns.into_iter().map(Into::into).collect();
        match self.body {
            InsertBody::Empty => {
                self.body = InsertBody::Bulk {
                    columns: cols,
                    rows: Vec::new(),
                };
            }
            InsertBody::Bulk { rows, .. } => {
                self.body = InsertBody::Bulk {
                    columns: cols,
                    rows,
                };
            }
            InsertBody::Select { body, .. } => {
                self.body = InsertBody::Select {
                    columns: cols,
                    body,
                };
            }
            InsertBody::Paired(_) => {
                panic!("InsertBuilder: `.columns()` cannot be mixed with `.value()`",)
            }
        }
        self
    }

    /// Add one row of bound values for the bulk form. Order must match the
    /// preceding [`columns`](Self::columns) call.
    pub fn row<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(InsertRow<'a>) -> InsertRow<'a>,
    {
        let row = callback(InsertRow::new());
        match &mut self.body {
            InsertBody::Bulk { rows, .. } => {
                rows.push(row);
            }
            InsertBody::Empty => {
                panic!("InsertBuilder: call `.columns([...])` before `.row(...)`",)
            }
            _ => panic!("InsertBuilder: `.row()` is only valid after `.columns([...])`",),
        }
        self
    }

    // ––– INSERT … SELECT –––

    /// Use a subquery (or UNNEST, VALUES, etc.) as the row source instead of
    /// a `VALUES` list. Closure pushes the SQL (and binds), typically a
    /// `SELECT …` or `SELECT * FROM UNNEST(…)`.
    ///
    /// Requires a preceding [`columns`](Self::columns) call.
    ///
    /// ```
    /// # use pgkit::query_builder::Query;
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "auth_permissions")] struct PermissionsRow { #[pgkit(primary_key)] id: i32, code: String, name: String, description: String, module: String }
    /// # use PermissionsColumn::*;
    /// # let (codes, names, descs, mods) = (vec!["a".to_string()], vec!["A".to_string()], vec!["".to_string()], vec!["m".to_string()]);
    /// let query = Query::insert().into("auth_permissions")
    ///     .columns([Code, Name, Description, Module])
    ///     .with_select(|q| {
    ///         q.push("SELECT * FROM UNNEST(");
    ///         q.push_bind(codes);  q.push("::text[], ");
    ///         q.push_bind(names);  q.push("::text[], ");
    ///         q.push_bind(descs);  q.push("::text[], ");
    ///         q.push_bind(mods);   q.push("::text[])");
    ///     })
    ///     .on_conflict([Code]).do_update(|u| u
    ///         .set_excluded(Name)
    ///         .set_excluded(Description))
    ///     .build();
    /// assert_eq!(
    ///     query.sql(),
    ///     "INSERT INTO auth_permissions (code, name, description, module) SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[]) ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name, description = EXCLUDED.description"
    /// );
    /// ```
    pub fn with_select<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        let bind: Binder<'a> = Box::new(callback);
        match self.body {
            InsertBody::Bulk { columns, .. } | InsertBody::Select { columns, .. } => {
                self.body = InsertBody::Select {
                    columns,
                    body: bind,
                };
            }
            InsertBody::Empty => {
                panic!("InsertBuilder: call `.columns([...])` before `.with_select(...)`",)
            }
            InsertBody::Paired(_) => {
                panic!("InsertBuilder: `.with_select()` cannot be mixed with `.value()`",)
            }
        }
        self
    }

    // ––– ON CONFLICT –––

    /// `ON CONFLICT (col1, col2, …)`: index-inferring conflict target.
    /// Chain `.do_nothing()` or `.do_update(|u| …)` to finish.
    pub fn on_conflict<C, I>(self, columns: I) -> OnConflict<'a>
    where
        C: Into<ColumnRef>,
        I: IntoIterator<Item = C>,
    {
        OnConflict {
            insert: self,
            target: ConflictTarget::Columns(columns.into_iter().map(Into::into).collect()),
        }
    }

    /// `ON CONFLICT ON CONSTRAINT <name>`: named-constraint conflict target.
    pub fn on_conflict_on_constraint(self, name: impl Into<String>) -> OnConflict<'a> {
        OnConflict {
            insert: self,
            target: ConflictTarget::Constraint(name.into()),
        }
    }

    /// `ON CONFLICT` with no target (matches any unique constraint).
    /// Only meaningful with `DO NOTHING`.
    pub fn on_conflict_any(self) -> OnConflict<'a> {
        OnConflict {
            insert: self,
            target: ConflictTarget::Any,
        }
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

    /// `RETURNING <raw-sql>`: escape hatch for expressions, casts, aliases.
    pub fn returning_raw(mut self, sql: impl Into<String>) -> Self {
        self.returning = Returning::Raw(sql.into());
        self
    }

    // ––– Terminal: render –––

    /// Render the INSERT into a fresh [`sqlx::QueryBuilder`]. Chain sqlx's
    /// `.build_query_as::<T>()` / `.fetch_one()` / etc. on the result.
    ///
    /// # Panics
    ///
    /// - If [`into`](Self::into) was never called.
    /// - If no values were declared (no `.value()`, no `.row()`, no `.with_select()`).
    pub fn build(self) -> QueryBuilder<Postgres> {
        let table = self
            .table
            .expect("InsertBuilder: missing `.into(table)` call");

        let mut query = sqlx::QueryBuilder::new(format!("INSERT INTO {table}"));

        match self.body {
            InsertBody::Empty => {
                panic!(
                    "InsertBuilder: no values declared; call `.value(...)`, `.row(...)`, or `.with_select(...)`",
                );
            }
            InsertBody::Paired(items) => {
                if items.is_empty() {
                    panic!("InsertBuilder: `.value(...)` was never called");
                }
                query.push(" (");
                for (i, (col, _)) in items.iter().enumerate() {
                    if i > 0 {
                        query.push(", ");
                    }
                    query.push(col.unqualified());
                }
                query.push(") VALUES (");
                for (i, (_, bind)) in items.into_iter().enumerate() {
                    if i > 0 {
                        query.push(", ");
                    }
                    bind(&mut query);
                }
                query.push(")");
            }
            InsertBody::Bulk { columns, rows } => {
                if columns.is_empty() {
                    panic!("InsertBuilder: `.columns([...])` was empty");
                }
                if rows.is_empty() {
                    panic!("InsertBuilder: no `.row(...)` calls after `.columns([...])`");
                }
                query.push(" (");
                for (i, col) in columns.iter().enumerate() {
                    if i > 0 {
                        query.push(", ");
                    }
                    query.push(col.unqualified());
                }
                query.push(") VALUES ");
                let col_count = columns.len();
                for (row_idx, row) in rows.into_iter().enumerate() {
                    assert_eq!(
                        row.len(),
                        col_count,
                        "InsertBuilder: row {} bound {} values but {} columns were declared",
                        row_idx,
                        row.len(),
                        col_count
                    );
                    if row_idx > 0 {
                        query.push(", ");
                    }
                    query.push("(");
                    for (i, bind) in row.binds.into_iter().enumerate() {
                        if i > 0 {
                            query.push(", ");
                        }
                        bind(&mut query);
                    }
                    query.push(")");
                }
            }
            InsertBody::Select { columns, body } => {
                if columns.is_empty() {
                    panic!("InsertBuilder: `.columns([...])` was empty");
                }
                query.push(" (");
                for (i, col) in columns.iter().enumerate() {
                    if i > 0 {
                        query.push(", ");
                    }
                    query.push(col.unqualified());
                }
                query.push(") ");
                body(&mut query);
            }
        }

        if let Some(on_conflict) = self.on_conflict {
            on_conflict.apply_to(&mut query);
        }

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

impl<'a> Default for InsertBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeColumn, FakeTable};

    #[test]
    fn paired_single_row() {
        let q = InsertBuilder::new()
            .into("geo_countries")
            .value("iso2", "UG")
            .value("name", "Uganda")
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO geo_countries (iso2, name) VALUES ($1, $2)"
        );
    }

    #[test]
    fn paired_with_returning_all() {
        let q = InsertBuilder::new()
            .into("geo_countries")
            .value("iso2", "UG")
            .returning_all()
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO geo_countries (iso2) VALUES ($1) RETURNING *"
        );
    }

    #[test]
    fn paired_with_returning_columns() {
        let q = InsertBuilder::new()
            .into("geo_countries")
            .value("iso2", "UG")
            .returning(["id", "iso2"])
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO geo_countries (iso2) VALUES ($1) RETURNING id, iso2"
        );
    }

    #[test]
    fn value_if_some_skips_none() {
        let q = InsertBuilder::new()
            .into("t")
            .value("a", 1)
            .value_if_some("b", None::<i32>)
            .value("c", 3)
            .build();

        assert_eq!(q.sql(), "INSERT INTO t (a, c) VALUES ($1, $2)");
    }

    #[test]
    fn value_raw_is_inlined() {
        let q = InsertBuilder::new()
            .into("t")
            .value("a", 1)
            .value_raw("created_at", |q| {
                q.push("NOW()");
            })
            .build();

        assert_eq!(q.sql(), "INSERT INTO t (a, created_at) VALUES ($1, NOW())");
    }

    #[test]
    fn bulk_multiple_rows() {
        let q = InsertBuilder::new()
            .into("t")
            .columns(["a", "b"])
            .row(|r| r.bind(1).bind("x"))
            .row(|r| r.bind(2).bind("y"))
            .row(|r| r.bind(3).bind("z"))
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO t (a, b) VALUES ($1, $2), ($3, $4), ($5, $6)"
        );
    }

    #[test]
    fn with_select_uses_subquery() {
        let q = InsertBuilder::new()
            .into("auth_permissions")
            .columns(["code", "name"])
            .with_select(|q| {
                q.push("SELECT * FROM UNNEST(");
                q.push_bind(vec!["a", "b"]);
                q.push("::text[], ");
                q.push_bind(vec!["A", "B"]);
                q.push("::text[])");
            })
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO auth_permissions (code, name) SELECT * FROM UNNEST($1::text[], $2::text[])"
        );
    }

    #[test]
    fn on_conflict_do_nothing() {
        let q = InsertBuilder::new()
            .into("t")
            .value("code", "x")
            .on_conflict(["code"])
            .do_nothing()
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO t (code) VALUES ($1) ON CONFLICT (code) DO NOTHING"
        );
    }

    #[test]
    fn on_conflict_do_update_set_excluded() {
        let q = InsertBuilder::new()
            .into("auth_permissions")
            .value("code", "x")
            .value("name", "X")
            .on_conflict(["code"])
            .do_update(|u| u.set_excluded("name").set_excluded("description"))
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO auth_permissions (code, name) VALUES ($1, $2) \
             ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name, description = EXCLUDED.description"
        );
    }

    #[test]
    fn on_conflict_do_update_set_value_and_raw() {
        let q = InsertBuilder::new()
            .into("t")
            .value("k", "key")
            .on_conflict(["k"])
            .do_update(|u| {
                u.set("v", 42).set_raw("updated_at", |q| {
                    q.push("NOW()");
                })
            })
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO t (k) VALUES ($1) \
             ON CONFLICT (k) DO UPDATE SET v = $2, updated_at = NOW()"
        );
    }

    #[test]
    fn on_conflict_on_constraint() {
        let q = InsertBuilder::new()
            .into("t")
            .value("a", 1)
            .on_conflict_on_constraint("uq_t_a")
            .do_nothing()
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO t (a) VALUES ($1) ON CONFLICT ON CONSTRAINT uq_t_a DO NOTHING"
        );
    }

    #[test]
    fn on_conflict_any() {
        let q = InsertBuilder::new()
            .into("t")
            .value("a", 1)
            .on_conflict_any()
            .do_nothing()
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO t (a) VALUES ($1) ON CONFLICT DO NOTHING"
        );
    }

    #[test]
    fn returning_raw() {
        let q = InsertBuilder::new()
            .into("t")
            .value("a", 1)
            .returning_raw("id, (a + 1) AS bumped")
            .build();

        assert_eq!(
            q.sql(),
            "INSERT INTO t (a) VALUES ($1) RETURNING id, (a + 1) AS bumped"
        );
    }

    #[test]
    fn aliased_table_is_rendered_with_alias() {
        let q = InsertBuilder::new()
            .into("geo_countries c")
            .value("iso2", "UG")
            .build();

        assert_eq!(q.sql(), "INSERT INTO geo_countries c (iso2) VALUES ($1)");
    }

    #[test]
    #[should_panic(expected = "missing `.into(table)`")]
    fn build_without_into_panics() {
        let _ = InsertBuilder::new().value("a", 1).build();
    }

    #[test]
    #[should_panic(expected = "no values declared")]
    fn build_without_values_panics() {
        let _ = InsertBuilder::new().into("t").build();
    }

    #[test]
    #[should_panic(expected = "cannot be mixed")]
    fn mixing_value_and_columns_panics() {
        let _ = InsertBuilder::new()
            .into("t")
            .value("a", 1)
            .columns(["a", "b"]);
    }

    #[test]
    fn typed_columns_in_value_render_unqualified() {
        let q = InsertBuilder::new()
            .into(FakeTable)
            .value(FakeColumn::Id, 1_i32)
            .value(FakeColumn::Name, "x")
            .build();
        assert_eq!(q.sql(), "INSERT INTO fake_table (id, name) VALUES ($1, $2)");
    }

    #[test]
    fn typed_columns_with_table_alias_still_render_unqualified_in_value() {
        let q = InsertBuilder::new()
            .into(FakeTable)
            .value(("ft", FakeColumn::Id), 1_i32)
            .build();
        assert_eq!(q.sql(), "INSERT INTO fake_table (id) VALUES ($1)");
    }

    #[test]
    fn typed_columns_in_bulk_form_render_unqualified() {
        let q = InsertBuilder::new()
            .into(FakeTable)
            .columns([FakeColumn::Id, FakeColumn::Name])
            .row(|r| r.bind(1_i32).bind("x"))
            .build();
        assert_eq!(q.sql(), "INSERT INTO fake_table (id, name) VALUES ($1, $2)");
    }

    #[test]
    fn typed_columns_in_on_conflict_target_render_unqualified() {
        let q = InsertBuilder::new()
            .into(FakeTable)
            .value(FakeColumn::Id, 1_i32)
            .on_conflict([FakeColumn::Id])
            .do_nothing()
            .build();
        assert_eq!(
            q.sql(),
            "INSERT INTO fake_table (id) VALUES ($1) ON CONFLICT (id) DO NOTHING"
        );
    }

    #[test]
    fn typed_columns_in_on_conflict_do_update_render_unqualified() {
        let q = InsertBuilder::new()
            .into(FakeTable)
            .value(FakeColumn::Id, 1_i32)
            .value(FakeColumn::Name, "x")
            .on_conflict([FakeColumn::Id])
            .do_update(|u| {
                u.set_excluded(FakeColumn::Name)
                    .set(FakeColumn::Id, 42_i32)
                    .set_raw(FakeColumn::Name, |q| {
                        q.push("'fallback'");
                    })
            })
            .build();
        assert_eq!(
            q.sql(),
            "INSERT INTO fake_table (id, name) VALUES ($1, $2) \
             ON CONFLICT (id) DO UPDATE SET \
             name = EXCLUDED.name, id = $3, name = 'fallback'"
        );
    }

    #[test]
    fn typed_columns_in_returning_render_unqualified() {
        let q = InsertBuilder::new()
            .into(FakeTable)
            .value(FakeColumn::Id, 1_i32)
            .returning([FakeColumn::Id, FakeColumn::Name])
            .build();
        assert_eq!(
            q.sql(),
            "INSERT INTO fake_table (id) VALUES ($1) RETURNING id, name"
        );
    }

    #[test]
    fn aliased_typed_columns_in_returning_render_unqualified_with_alias() {
        let q = InsertBuilder::new()
            .into(FakeTable)
            .value(FakeColumn::Id, 1_i32)
            .returning([ColumnRef::from((FakeColumn::Id, "fake_id"))])
            .build();
        assert_eq!(
            q.sql(),
            "INSERT INTO fake_table (id) VALUES ($1) RETURNING id AS fake_id"
        );
    }
}
