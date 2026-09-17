use sqlx::{Postgres, QueryBuilder};

use super::cte::{CteBody, CteItem};
use super::distinct::Distinct;
use super::join_item::JoinItem;
use super::join_kind::JoinKind;
use super::join_on::JoinOn;
use super::locking::Locking;
use super::locking_clause::LockingClause;
use super::nulls_order::NullsOrder;
use super::order_item::OrderItem;
use super::select_item::SelectItem;
use super::union_kind::UnionKind;
use crate::ordering::OrderByDirection;
use crate::projection::ColumnSelection;
use crate::query_builder::exec::{BuildQuery, BuildQueryAs, BuildQueryScalar};
use crate::query_builder::ops::HasWhere;
use crate::query_builder::where_builder::WhereBuilder;
use crate::types::{ColumnRef, DatabaseTable, DatabaseTableColumn, TableRef};

type ExprFn<'a> = Box<dyn FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a>;

/// Fluent builder for PostgreSQL `SELECT` statements.
///
/// Covers the common surface: columns + expressions, aliases, DISTINCT /
/// DISTINCT ON, JOINs (inner / left / right / full / cross), WHERE
/// (via [`WhereOps`](crate::query_builder::WhereOps)), GROUP BY, HAVING,
/// ORDER BY, LIMIT / OFFSET, UNION / UNION ALL, FOR UPDATE / SHARE,
/// and basic `WITH` CTEs.
///
/// Beyond what's exposed here, [`expr_with`](Self::expr_with) /
/// [`where_raw_with`](crate::query_builder::WhereOps::where_raw_with) /
/// [`from_raw`](Self::from_raw) are escape hatches for anything not
/// modelled directly.
///
/// ```no_run
/// # #[derive(sqlx::FromRow, pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, iso2: String, name: String, deleted_at: Option<chrono::DateTime<chrono::Utc>> }
/// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "addresses")] struct AddressesRow { #[pgkit(primary_key)] id: i32, country_id: i32, owner_id: i64, deleted_at: Option<chrono::DateTime<chrono::Utc>> }
/// # #[derive(sqlx::FromRow)] struct AddressRow { id: i32, country_name: String }
/// # async fn demo(pool: sqlx::PgPool, owner_id: i64) -> Result<(), Box<dyn std::error::Error>> {
/// use pgkit::query_builder::{Query, WhereOps};
///
/// let row = Query::select()
///     .from((AddressesTable, "a"))
///     .column(("a", AddressesColumn::Id))
///     .column_as(("c", CountriesColumn::Name), "country_name")
///     .inner_join((CountriesTable, "c"),
///         |on| on.eq(("a", AddressesColumn::CountryId), ("c", CountriesColumn::Id)))
///     .where_eq(("a", AddressesColumn::OwnerId), owner_id)
///     .where_null(("a", AddressesColumn::DeletedAt))
///     .build_query_as::<AddressRow>()
///     .fetch_optional(&pool).await?;
/// # Ok(()) }
/// ```
pub struct SelectBuilder<'a> {
    ctes: Vec<CteItem<'a>>,
    distinct: Distinct,
    select_list: Vec<SelectItem<'a>>,
    from: Vec<TableRef>,
    from_raw: Option<ExprFn<'a>>,
    joins: Vec<JoinItem<'a>>,
    where_clause: WhereBuilder<'a>,
    group_by: Vec<ColumnRef>,
    group_by_raw: Vec<String>,
    having: WhereBuilder<'a>,
    order_by: Vec<OrderItem>,
    limit: Option<i64>,
    offset: Option<i64>,
    unions: Vec<(UnionKind, Box<SelectBuilder<'a>>)>,
    locking: Option<LockingClause>,
    /// Wraps the rendered SELECT in `SELECT EXISTS(<select>)`.
    exists_wrap: bool,
}

impl<'a> SelectBuilder<'a> {
    /// Empty builder. Prefer [`Query::select()`](crate::query_builder::Query::select).
    pub fn new() -> Self {
        Self {
            ctes: Vec::new(),
            distinct: Distinct::None,
            select_list: Vec::new(),
            from: Vec::new(),
            from_raw: None,
            joins: Vec::new(),
            where_clause: WhereBuilder::new(),
            group_by: Vec::new(),
            group_by_raw: Vec::new(),
            having: WhereBuilder::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            unions: Vec::new(),
            locking: None,
            exists_wrap: false,
        }
    }

    // ––– FROM –––

    /// `FROM <table>`: primary source. Repeated calls add comma-separated
    /// tables (implicit CROSS JOIN).
    pub fn from<T: Into<TableRef>>(mut self, table: T) -> Self {
        self.from.push(table.into());
        self
    }

    /// `FROM <raw>`: escape hatch for subqueries, `VALUES`, `LATERAL`, etc.
    /// Mutually exclusive with [`from`](Self::from).
    pub fn from_raw<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.from_raw = Some(Box::new(callback));
        self
    }

    // ––– SELECT list –––

    /// Add `*` to the SELECT list.
    pub fn all(mut self) -> Self {
        self.select_list.push(SelectItem::All);
        self
    }

    /// Add `<qualifier>.*` to the SELECT list, e.g. `.all_of("c")`.
    pub fn all_of(mut self, qualifier: impl Into<String>) -> Self {
        self.select_list.push(SelectItem::AllOf(qualifier.into()));
        self
    }

    /// Add a single column to the SELECT list.
    pub fn column<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        self.select_list.push(SelectItem::Column(column.into()));
        self
    }

    /// Add `<column> AS <alias>`.
    pub fn column_as<C: Into<ColumnRef>>(mut self, column: C, alias: impl Into<String>) -> Self {
        self.select_list
            .push(SelectItem::ColumnAs(column.into(), alias.into()));
        self
    }

    /// Add several columns at once.
    pub fn columns<C, I>(mut self, columns: I) -> Self
    where
        C: Into<ColumnRef>,
        I: IntoIterator<Item = C>,
    {
        for c in columns {
            self.select_list.push(SelectItem::Column(c.into()));
        }
        self
    }

    /// Add every column declared on the [`DatabaseTableColumn`] type,
    /// qualified by the type's table name (e.g. `geo_countries.id,
    /// geo_countries.iso2, …`). Mirrors
    /// [`DatabaseTableColumn::select_all`].
    pub fn all_columns_of<C: DatabaseTableColumn>(mut self) -> Self {
        for c in C::iter() {
            self.select_list
                .push(SelectItem::Column(ColumnRef::from(c)));
        }
        self
    }

    /// Like [`all_columns_of`](Self::all_columns_of) but qualified by the
    /// given **table alias** (e.g. `c.id, c.iso2, …` when `table_alias = "c"`).
    pub fn all_columns_of_with_table_alias<C: DatabaseTableColumn>(
        mut self,
        table_alias: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Self {
        let alias = table_alias.into();
        for c in C::iter() {
            self.select_list
                .push(SelectItem::Column(ColumnRef::Qualified {
                    qualifier: alias.clone(),
                    column: c.into(),
                }));
        }
        self
    }

    /// Add a raw SQL expression to the SELECT list, e.g. `COUNT(*)`,
    /// `COALESCE(a, b)`, `EXTRACT(EPOCH FROM x)`.
    pub fn expr(mut self, sql: impl Into<String>) -> Self {
        self.select_list.push(SelectItem::Expr(sql.into()));
        self
    }

    /// Raw expression with alias: `<sql> AS <alias>`.
    pub fn expr_as(mut self, sql: impl Into<String>, alias: impl Into<String>) -> Self {
        self.select_list
            .push(SelectItem::ExprAs(sql.into(), alias.into()));
        self
    }

    /// Deferred expression: closure pushes its own SQL and parameter binds.
    /// Use for expressions that need bound values, e.g.
    /// `to_tsquery('simple', $1)`.
    pub fn expr_with<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.select_list
            .push(SelectItem::ExprWith(Box::new(callback)));
        self
    }

    /// Deferred expression with alias.
    pub fn expr_with_as<F>(mut self, callback: F, alias: impl Into<String>) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.select_list
            .push(SelectItem::ExprWithAs(Box::new(callback), alias.into()));
        self
    }

    // ––– DISTINCT shortcuts –––

    /// `SELECT DISTINCT …`.
    pub fn distinct(mut self) -> Self {
        self.distinct = Distinct::All;
        self
    }

    /// `SELECT DISTINCT ON (col1, col2, …) …`.
    pub fn distinct_on<C, I>(mut self, columns: I) -> Self
    where
        C: Into<ColumnRef>,
        I: IntoIterator<Item = C>,
    {
        self.distinct = Distinct::On(columns.into_iter().map(Into::into).collect());
        self
    }

    // ––– Aggregate shortcuts –––

    /// `SELECT COUNT(*)`. Clears any existing select list.
    pub fn count(mut self) -> Self {
        self.select_list.clear();
        self.select_list.push(SelectItem::Expr("COUNT(*)".into()));
        self
    }

    /// `SELECT COUNT(<column>)`. Clears any existing select list.
    pub fn count_of<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        let col = column.into();
        self.select_list.clear();
        self.select_list
            .push(SelectItem::Expr(format!("COUNT({col})")));
        self
    }

    /// `SELECT COUNT(DISTINCT <column>)`. Clears any existing select list.
    pub fn count_distinct<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        let col = column.into();
        self.select_list.clear();
        self.select_list
            .push(SelectItem::Expr(format!("COUNT(DISTINCT {col})")));
        self
    }

    /// Wraps the final SQL in `SELECT EXISTS(<select>)`. The inner select
    /// list is replaced with `1`. Pairs with `.build_query_scalar::<bool>()`.
    pub fn exists(mut self) -> Self {
        self.select_list.clear();
        self.select_list.push(SelectItem::Expr("1".into()));
        self.exists_wrap = true;
        self
    }

    // ––– JOINs –––

    fn join_with<T, F>(mut self, kind: JoinKind, table: T, on: F) -> Self
    where
        T: Into<TableRef>,
        F: FnOnce(JoinOn<'a>) -> JoinOn<'a>,
    {
        let on = on(JoinOn::new());
        assert!(
            !on.is_empty(),
            "{} requires a non-empty ON predicate (use cross_join for no ON)",
            kind.keyword(),
        );
        self.joins.push(JoinItem {
            kind,
            table: table.into(),
            on: Some(on),
        });
        self
    }

    /// `INNER JOIN <table> ON <on>`.
    pub fn inner_join<T, F>(self, table: T, on: F) -> Self
    where
        T: Into<TableRef>,
        F: FnOnce(JoinOn<'a>) -> JoinOn<'a>,
    {
        self.join_with(JoinKind::Inner, table, on)
    }

    /// `LEFT JOIN <table> ON <on>`.
    pub fn left_join<T, F>(self, table: T, on: F) -> Self
    where
        T: Into<TableRef>,
        F: FnOnce(JoinOn<'a>) -> JoinOn<'a>,
    {
        self.join_with(JoinKind::Left, table, on)
    }

    /// `RIGHT JOIN <table> ON <on>`.
    pub fn right_join<T, F>(self, table: T, on: F) -> Self
    where
        T: Into<TableRef>,
        F: FnOnce(JoinOn<'a>) -> JoinOn<'a>,
    {
        self.join_with(JoinKind::Right, table, on)
    }

    /// `FULL JOIN <table> ON <on>`.
    pub fn full_join<T, F>(self, table: T, on: F) -> Self
    where
        T: Into<TableRef>,
        F: FnOnce(JoinOn<'a>) -> JoinOn<'a>,
    {
        self.join_with(JoinKind::Full, table, on)
    }

    /// `CROSS JOIN <table>` (no ON predicate).
    pub fn cross_join<T: Into<TableRef>>(mut self, table: T) -> Self {
        self.joins.push(JoinItem {
            kind: JoinKind::Cross,
            table: table.into(),
            on: None,
        });
        self
    }

    // ––– Embedding a related resource –––

    fn including<C2, F>(mut self, columns: ColumnSelection<C2>, kind: JoinKind, on: F) -> Self
    where
        C2: DatabaseTableColumn,
        F: FnOnce(JoinOn<'a>) -> JoinOn<'a>,
    {
        let table = <C2::Table as DatabaseTable>::NAME;
        for col in columns {
            let name: &'static str = col.into();
            self = self.column_as(col, format!("{table}_{name}"));
        }
        self.join_with(kind, table, on)
    }

    /// Fetch a related resource's columns using `INNER JOIN`.
    pub fn include<C2, L>(self, columns: ColumnSelection<C2>, on_left: L, on_right: C2) -> Self
    where
        C2: DatabaseTableColumn,
        L: Into<ColumnRef>,
    {
        let soft_delete = <C2::Table as DatabaseTable>::soft_delete_column();
        self.including(columns, JoinKind::Inner, move |on| {
            let on = on.eq(on_left, on_right);
            match soft_delete {
                Some(deleted_at) => on.null(deleted_at),
                None => on,
            }
        })
    }

    /// [`include`](Self::include) only when `condition` is true.
    pub fn include_if<C2, L>(
        self,
        condition: bool,
        columns: ColumnSelection<C2>,
        on_left: L,
        on_right: C2,
    ) -> Self
    where
        C2: DatabaseTableColumn,
        L: Into<ColumnRef>,
    {
        if condition {
            return self.include(columns, on_left, on_right);
        }
        self
    }

    /// [`include`](Self::include) only when `columns` is `Some`.
    pub fn include_if_some<C2, L>(
        self,
        columns: Option<ColumnSelection<C2>>,
        on_left: L,
        on_right: C2,
    ) -> Self
    where
        C2: DatabaseTableColumn,
        L: Into<ColumnRef>,
    {
        if let Some(columns) = columns {
            return self.include(columns, on_left, on_right);
        }
        self
    }

    /// Fetch a related resource's columns using `LEFT JOIN`.
    /// (Related resource might be absent)
    pub fn include_nullable<C2, L>(
        self,
        columns: ColumnSelection<C2>,
        on_left: L,
        on_right: C2,
    ) -> Self
    where
        C2: DatabaseTableColumn,
        L: Into<ColumnRef>,
    {
        let soft_delete = <C2::Table as DatabaseTable>::soft_delete_column();
        self.including(columns, JoinKind::Left, move |on| {
            let on = on.eq(on_left, on_right);
            match soft_delete {
                Some(deleted_at) => on.null(deleted_at),
                None => on,
            }
        })
    }

    /// [`include_nullable`](Self::include_nullable) only when `condition` is true.
    pub fn include_nullable_if<C2, L>(
        self,
        condition: bool,
        columns: ColumnSelection<C2>,
        on_left: L,
        on_right: C2,
    ) -> Self
    where
        C2: DatabaseTableColumn,
        L: Into<ColumnRef>,
    {
        if condition {
            return self.include_nullable(columns, on_left, on_right);
        }
        self
    }

    /// [`include_nullable`](Self::include_nullable) only when `columns` is `Some`.
    pub fn include_nullable_if_some<C2, L>(
        self,
        columns: Option<ColumnSelection<C2>>,
        on_left: L,
        on_right: C2,
    ) -> Self
    where
        C2: DatabaseTableColumn,
        L: Into<ColumnRef>,
    {
        if let Some(columns) = columns {
            return self.include_nullable(columns, on_left, on_right);
        }
        self
    }

    /// Like [`include`](Self::include) but you build the `ON` predicate.
    pub fn include_with<C2, F>(self, columns: ColumnSelection<C2>, on: F) -> Self
    where
        C2: DatabaseTableColumn,
        F: FnOnce(JoinOn<'a>) -> JoinOn<'a>,
    {
        self.including(columns, JoinKind::Inner, on)
    }

    /// Like [`include_nullable`](Self::include_nullable) but you build the `ON` predicate.
    pub fn include_nullable_with<C2, F>(self, columns: ColumnSelection<C2>, on: F) -> Self
    where
        C2: DatabaseTableColumn,
        F: FnOnce(JoinOn<'a>) -> JoinOn<'a>,
    {
        self.including(columns, JoinKind::Left, on)
    }

    // ––– GROUP BY –––

    pub fn group_by<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        self.group_by.push(column.into());
        self
    }

    pub fn group_by_cols<C, I>(mut self, columns: I) -> Self
    where
        C: Into<ColumnRef>,
        I: IntoIterator<Item = C>,
    {
        self.group_by.extend(columns.into_iter().map(Into::into));
        self
    }

    /// `GROUP BY <sql>`: escape hatch for expressions, GROUPING SETS, ROLLUP.
    pub fn group_by_raw(mut self, sql: impl Into<String>) -> Self {
        self.group_by_raw.push(sql.into());
        self
    }

    // ––– HAVING –––

    /// Apply a closure to the HAVING [`WhereBuilder`], gaining the full
    /// predicate API for the HAVING clause.
    pub fn having<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(WhereBuilder<'a>) -> WhereBuilder<'a>,
    {
        let w = std::mem::take(&mut self.having);
        self.having = callback(w);
        self
    }

    /// `HAVING <raw>`: escape hatch.
    pub fn having_raw(mut self, sql: &str) -> Self {
        let w = std::mem::take(&mut self.having);
        self.having = w.where_raw(sql);
        self
    }

    // ––– ORDER BY –––

    /// `ORDER BY col <dir>`.
    pub fn order_by<C: Into<ColumnRef>>(mut self, column: C, dir: OrderByDirection) -> Self {
        self.order_by.push(OrderItem::Column {
            col: column.into(),
            dir,
            nulls: None,
        });
        self
    }

    /// `ORDER BY col <dir> [NULLS FIRST|LAST]`.
    pub fn order_by_nulls<C: Into<ColumnRef>>(
        mut self,
        column: C,
        dir: OrderByDirection,
        nulls: NullsOrder,
    ) -> Self {
        self.order_by.push(OrderItem::Column {
            col: column.into(),
            dir,
            nulls: Some(nulls),
        });
        self
    }

    /// Raw ORDER BY fragment: for expressions like `LOWER(name) ASC`.
    pub fn order_by_raw(mut self, sql: impl Into<String>) -> Self {
        self.order_by.push(OrderItem::Raw(sql.into()));
        self
    }

    // ––– LIMIT / OFFSET –––

    /// `LIMIT <n>`: bound as a parameter.
    pub fn limit(mut self, n: i64) -> Self {
        self.limit = Some(n);
        self
    }

    /// `OFFSET <n>`: bound as a parameter.
    pub fn offset(mut self, n: i64) -> Self {
        self.offset = Some(n);
        self
    }

    /// Apply offset pagination: `LIMIT <page_size> OFFSET <(page-1) * page_size>`.
    ///
    /// # Example
    ///
    /// ```
    /// # use pgkit::query_builder::WhereOps;
    /// # #[derive(sqlx::FromRow, pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, iso2: String, name: String, deleted_at: Option<chrono::DateTime<chrono::Utc>> }
    /// use pgkit::query_builder::Query;
    /// use pgkit::pagination::offset::OffsetPagination;
    ///
    /// let q = Query::select()
    ///     .from(CountriesTable)
    ///     .all()
    ///     .where_null(CountriesColumn::DeletedAt)
    ///     .paginate(&OffsetPagination::new(2, 20))
    ///     .build();
    /// assert_eq!(
    ///     q.sql(),
    ///     "SELECT * FROM geo_countries WHERE geo_countries.deleted_at IS NULL LIMIT $1 OFFSET $2"
    /// );
    /// ```
    #[cfg(feature = "offset-pagination")]
    pub fn paginate(self, pagination: &crate::pagination::offset::OffsetPagination) -> Self {
        self.limit(i64::from(pagination.limit()))
            .offset(pagination.offset() as i64)
    }

    /// Apply cursor pagination: adds the keyset WHERE predicate (if the
    /// cursor is present), an `ORDER BY <col>, <pk>` tiebreaker, and a
    /// `LIMIT` of [`crate::pagination::cursor::CursorPagination::fetch_limit`].
    ///
    /// # Example
    ///
    /// ```
    /// # use pgkit::query_builder::WhereOps;
    /// # use pgkit::ordering::{OrderBy, OrderByDirection};
    /// # use pgkit::pagination::cursor::CursorPagination;
    /// # #[derive(sqlx::FromRow, pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, iso2: String, name: String, deleted_at: Option<chrono::DateTime<chrono::Utc>> }
    /// use pgkit::query_builder::Query;
    ///
    /// // Normally built from the request's `order_by` / `limit` / `cursor` params.
    /// let pagination = CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
    ///     OrderBy::new(CountriesColumn::Name, OrderByDirection::Asc),
    ///     20,
    ///     None,
    ///     None,
    /// )?;
    ///
    /// let q = Query::select()
    ///     .from(CountriesTable)
    ///     .all_columns_of::<CountriesColumn>()
    ///     .where_null(CountriesColumn::DeletedAt)
    ///     .cursor_paginate(&pagination)
    ///     .build();
    /// assert_eq!(
    ///     q.sql(),
    ///     "SELECT geo_countries.id, geo_countries.iso2, geo_countries.name, geo_countries.deleted_at FROM geo_countries WHERE geo_countries.deleted_at IS NULL ORDER BY geo_countries.name ASC NULLS LAST, geo_countries.id ASC NULLS LAST LIMIT $1"
    /// );
    /// # Ok::<(), pgkit::pagination::cursor::CursorPaginationError>(())
    /// ```
    #[cfg(feature = "cursor-pagination")]
    pub fn cursor_paginate<T, Id>(
        self,
        pagination: &crate::pagination::cursor::CursorPagination<T, Id>,
    ) -> Self
    where
        T: DatabaseTableColumn + crate::types::ColumnTypeInfo + Copy,
        Id: sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres> + Send + Clone + 'a,
    {
        use crate::pagination::cursor::{CursorValue, push_keyset_predicate};
        use crate::query_builder::WhereOps;

        let (col, sort_dir) = pagination.order_by();
        let col_ref = ColumnRef::from(col);
        let id_col_ref = ColumnRef::from(T::table_primary_key());
        let nulls = NullsOrder::from(sort_dir);

        let with_keyset = if let Some(cursor) = pagination.cursor() {
            let col_for_pred = col_ref.clone();
            let id_col_for_pred = id_col_ref.clone();
            let cursor_dir = cursor.dir;
            let val = cursor.val.clone();
            let id = cursor.id.clone();
            // Cast target: only when the carried value is an enum label. The
            // Postgres type name comes from the column's own Rust type,
            // server-side, trusted; never from the cursor.
            let cast = match &val {
                Some(CursorValue::Enum(_)) => Some(col.pg_type_info()),
                _ => None,
            };
            self.map_where(|w| {
                w.where_raw_with(move |qb| {
                    push_keyset_predicate(
                        qb,
                        &col_for_pred,
                        &id_col_for_pred,
                        cursor_dir,
                        val,
                        id,
                        cast,
                    );
                })
            })
        } else {
            self
        };

        with_keyset
            .order_by_nulls(col_ref, sort_dir, nulls)
            .order_by_nulls(id_col_ref, sort_dir, nulls)
            .limit(pagination.fetch_limit())
    }

    // ––– UNION –––

    /// `… UNION <other>`.
    pub fn union(mut self, other: SelectBuilder<'a>) -> Self {
        self.unions.push((UnionKind::Distinct, Box::new(other)));
        self
    }

    /// `… UNION ALL <other>`.
    pub fn union_all(mut self, other: SelectBuilder<'a>) -> Self {
        self.unions.push((UnionKind::All, Box::new(other)));
        self
    }

    // ––– Row locking –––

    /// `FOR UPDATE`.
    pub fn for_update(mut self) -> Self {
        self.locking = Some(LockingClause {
            kind: Locking::Update,
            of: Vec::new(),
            nowait: false,
            skip_locked: false,
        });
        self
    }

    /// `FOR SHARE`.
    pub fn for_share(mut self) -> Self {
        self.locking = Some(LockingClause {
            kind: Locking::Share,
            of: Vec::new(),
            nowait: false,
            skip_locked: false,
        });
        self
    }

    /// `FOR NO KEY UPDATE`.
    pub fn for_no_key_update(mut self) -> Self {
        self.locking = Some(LockingClause {
            kind: Locking::NoKeyUpdate,
            of: Vec::new(),
            nowait: false,
            skip_locked: false,
        });
        self
    }

    /// `FOR KEY SHARE`.
    pub fn for_key_share(mut self) -> Self {
        self.locking = Some(LockingClause {
            kind: Locking::KeyShare,
            of: Vec::new(),
            nowait: false,
            skip_locked: false,
        });
        self
    }

    /// `OF <table>` modifier on the active locking clause. Call after
    /// [`for_update`](Self::for_update) / [`for_share`](Self::for_share) / etc.
    pub fn lock_of<T: Into<TableRef>>(mut self, table: T) -> Self {
        if let Some(lock) = self.locking.as_mut() {
            lock.of.push(table.into());
        }
        self
    }

    /// `NOWAIT`. Call after `for_*`.
    pub fn nowait(mut self) -> Self {
        if let Some(lock) = self.locking.as_mut() {
            lock.nowait = true;
        }
        self
    }

    /// `SKIP LOCKED`. Call after `for_*`.
    pub fn skip_locked(mut self) -> Self {
        if let Some(lock) = self.locking.as_mut() {
            lock.skip_locked = true;
        }
        self
    }

    // ––– CTEs –––

    /// `WITH <name> AS (<select>)`: add a CTE backed by another [`SelectBuilder`].
    pub fn with_cte(mut self, name: impl Into<String>, select: SelectBuilder<'a>) -> Self {
        self.ctes.push(CteItem {
            name: name.into(),
            recursive: false,
            body: CteBody::Select(Box::new(select)),
        });
        self
    }

    /// `WITH RECURSIVE <name> AS (<sql>)`: recursive CTE with raw body.
    pub fn with_recursive<F>(mut self, name: impl Into<String>, body: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.ctes.push(CteItem {
            name: name.into(),
            recursive: true,
            body: CteBody::Raw(Box::new(body)),
        });
        self
    }

    /// `WITH <name> AS (<raw sql>)`: CTE with a hand-written body.
    pub fn with_cte_raw<F>(mut self, name: impl Into<String>, body: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.ctes.push(CteItem {
            name: name.into(),
            recursive: false,
            body: CteBody::Raw(Box::new(body)),
        });
        self
    }

    // ––– Terminal: render –––

    /// Render into a fresh [`sqlx::QueryBuilder`]. Chain sqlx terminals on the result.
    pub fn build(self) -> QueryBuilder<Postgres> {
        let mut query = sqlx::QueryBuilder::new(String::new());
        self.apply_to(&mut query);
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

    /// Apply this SELECT to an existing [`sqlx::QueryBuilder`], useful for
    /// embedding as a subquery (e.g., `INSERT … SELECT`, `WHERE x IN (…)`).
    pub fn apply_to(self, query: &mut QueryBuilder<Postgres>) {
        if self.exists_wrap {
            query.push("SELECT EXISTS (");
            self.render_core(query);
            query.push(")");
        } else {
            self.render_core(query);
        }
    }

    fn render_core(self, query: &mut QueryBuilder<Postgres>) {
        // WITH
        if !self.ctes.is_empty() {
            let any_recursive = self.ctes.iter().any(|c| c.recursive);
            if any_recursive {
                query.push("WITH RECURSIVE ");
            } else {
                query.push("WITH ");
            }
            for (i, cte) in self.ctes.into_iter().enumerate() {
                if i > 0 {
                    query.push(", ");
                }
                cte.apply_to(query);
            }
            query.push(" ");
        }

        // SELECT
        query.push("SELECT ");
        self.distinct.apply_to(query);

        if self.select_list.is_empty() {
            query.push("*");
        } else {
            for (i, item) in self.select_list.into_iter().enumerate() {
                if i > 0 {
                    query.push(", ");
                }
                item.apply_to(query);
            }
        }

        // FROM
        if let Some(from_raw) = self.from_raw {
            query.push(" FROM ");
            from_raw(query);
        } else if !self.from.is_empty() {
            query.push(" FROM ");
            for (i, t) in self.from.iter().enumerate() {
                if i > 0 {
                    query.push(", ");
                }
                query.push(t);
            }
        }

        // JOINs
        for j in self.joins {
            j.apply_to(query);
        }

        // WHERE
        self.where_clause.apply_to(query);

        // GROUP BY
        if !self.group_by.is_empty() || !self.group_by_raw.is_empty() {
            query.push(" GROUP BY ");
            let mut first = true;
            for c in &self.group_by {
                if !first {
                    query.push(", ");
                }
                query.push(c);
                first = false;
            }
            for raw in &self.group_by_raw {
                if !first {
                    query.push(", ");
                }
                query.push(raw);
                first = false;
            }
        }

        // HAVING
        if !self.having.is_empty() {
            query.push(" HAVING ");
            self.having.apply_predicate(query);
        }

        // UNIONs: arms are parenthesized so an arm carrying its own
        // ORDER BY / LIMIT stays valid SQL.
        for (kind, other) in self.unions {
            query.push(kind.keyword());
            query.push("(");
            other.apply_to(query);
            query.push(")");
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            query.push(" ORDER BY ");
            for (i, item) in self.order_by.into_iter().enumerate() {
                if i > 0 {
                    query.push(", ");
                }
                item.apply_to(query);
            }
        }

        // LIMIT / OFFSET
        if let Some(n) = self.limit {
            query.push(" LIMIT ");
            query.push_bind(n);
        }
        if let Some(n) = self.offset {
            query.push(" OFFSET ");
            query.push_bind(n);
        }

        // Locking
        if let Some(lock) = self.locking {
            lock.apply_to(query);
        }
    }
}

impl<'a> Default for SelectBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> HasWhere<'a> for SelectBuilder<'a> {
    fn where_mut(&mut self) -> &mut WhereBuilder<'a> {
        &mut self.where_clause
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ordering::OrderByDirection::*;
    use crate::projection::ColumnSelection;
    use crate::query_builder::ops::WhereOps;
    use crate::test_support::{FakeColumn, FakeRelatedColumn, FakeSoftColumn, FakeTable};

    #[test]
    fn select_star_from_table() {
        let q = SelectBuilder::new().from("t").build();
        assert_eq!(q.sql(), "SELECT * FROM t");
    }

    #[test]
    fn select_columns_from_table() {
        let q = SelectBuilder::new()
            .from("t")
            .columns(["id", "name"])
            .build();
        assert_eq!(q.sql(), "SELECT id, name FROM t");
    }

    #[test]
    fn select_with_where() {
        let q = SelectBuilder::new()
            .from("t")
            .columns(["id"])
            .where_eq("id", 1_i32)
            .build();
        assert_eq!(q.sql(), "SELECT id FROM t WHERE id = $1");
    }

    #[test]
    fn select_with_alias_and_column_as() {
        let q = SelectBuilder::new()
            .from("geo_countries c")
            .column("c.id")
            .column_as("c.name", "country_name")
            .build();
        assert_eq!(
            q.sql(),
            "SELECT c.id, c.name AS country_name FROM geo_countries c"
        );
    }

    #[test]
    fn all_columns_of_with_table_alias_accepts_static_and_runtime_aliases() {
        let expected = "SELECT f.id, f.name FROM fake_table f";

        let from_static = SelectBuilder::new()
            .from("fake_table f")
            .all_columns_of_with_table_alias::<FakeColumn>("f")
            .build();
        assert_eq!(from_static.sql(), expected);

        let runtime_alias = String::from("f");
        let from_runtime = SelectBuilder::new()
            .from("fake_table f")
            .all_columns_of_with_table_alias::<FakeColumn>(runtime_alias)
            .build();
        assert_eq!(from_runtime.sql(), expected);
    }

    #[test]
    fn select_with_inner_join() {
        let q = SelectBuilder::new()
            .from("geo_addresses a")
            .column("a.id")
            .column_as("c.name", "country_name")
            .inner_join("geo_countries c", |on| on.eq("a.country_id", "c.id"))
            .where_eq("a.owner_id", 1_i32)
            .build();
        assert_eq!(
            q.sql(),
            "SELECT a.id, c.name AS country_name FROM geo_addresses a \
             INNER JOIN geo_countries c ON a.country_id = c.id \
             WHERE a.owner_id = $1"
        );
    }

    #[test]
    fn select_with_left_and_right_joins() {
        let q = SelectBuilder::new()
            .from("t a")
            .all()
            .left_join("u b", |on| on.eq("a.id", "b.aid"))
            .right_join("v c", |on| on.eq("a.id", "c.aid"))
            .build();
        assert_eq!(
            q.sql(),
            "SELECT * FROM t a LEFT JOIN u b ON a.id = b.aid RIGHT JOIN v c ON a.id = c.aid"
        );
    }

    #[test]
    fn select_cross_join() {
        let q = SelectBuilder::new()
            .from("t a")
            .all()
            .cross_join("u b")
            .build();
        assert_eq!(q.sql(), "SELECT * FROM t a CROSS JOIN u b");
    }

    #[test]
    fn include_projects_prefixed_columns_and_inner_joins() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all_columns_of::<FakeColumn>()
            .include(
                ColumnSelection::new([FakeRelatedColumn::Label]),
                "fake_table.related_id",
                FakeRelatedColumn::Id,
            )
            .build();
        // ColumnSelection injects the related PK; columns are aliased
        // `<table>_<column>` and the relation is INNER JOINed on its table name.
        assert_eq!(
            q.sql(),
            "SELECT fake_table.id, fake_table.name, \
             fake_related.id AS fake_related_id, fake_related.label AS fake_related_label \
             FROM fake_table \
             INNER JOIN fake_related ON fake_table.related_id = fake_related.id"
        );
    }

    #[test]
    fn include_nullable_uses_left_join() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all_columns_of::<FakeColumn>()
            .include_nullable(
                ColumnSelection::new([FakeRelatedColumn::Label]),
                "fake_table.related_id",
                FakeRelatedColumn::Id,
            )
            .build();
        assert_eq!(
            q.sql(),
            "SELECT fake_table.id, fake_table.name, \
             fake_related.id AS fake_related_id, fake_related.label AS fake_related_label \
             FROM fake_table \
             LEFT JOIN fake_related ON fake_table.related_id = fake_related.id"
        );
    }

    #[test]
    fn include_if_false_is_a_noop() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_if(
                false,
                ColumnSelection::new([FakeRelatedColumn::Label]),
                "fake_table.related_id",
                FakeRelatedColumn::Id,
            )
            .build();
        assert_eq!(q.sql(), "SELECT * FROM fake_table");
    }

    #[test]
    fn include_if_some_none_is_a_noop() {
        let none: Option<ColumnSelection<FakeRelatedColumn>> = None;
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_if_some(none, "fake_table.related_id", FakeRelatedColumn::Id)
            .build();
        assert_eq!(q.sql(), "SELECT * FROM fake_table");
    }

    #[test]
    fn include_if_true_inner_joins() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_if(
                true,
                ColumnSelection::new([FakeRelatedColumn::Label]),
                "fake_table.related_id",
                FakeRelatedColumn::Id,
            )
            .build();
        assert_eq!(
            q.sql(),
            "SELECT *, fake_related.id AS fake_related_id, \
             fake_related.label AS fake_related_label \
             FROM fake_table \
             INNER JOIN fake_related ON fake_table.related_id = fake_related.id"
        );
    }

    #[test]
    fn include_if_some_some_inner_joins() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_if_some(
                Some(ColumnSelection::new([FakeRelatedColumn::Label])),
                "fake_table.related_id",
                FakeRelatedColumn::Id,
            )
            .build();
        assert_eq!(
            q.sql(),
            "SELECT *, fake_related.id AS fake_related_id, \
             fake_related.label AS fake_related_label \
             FROM fake_table \
             INNER JOIN fake_related ON fake_table.related_id = fake_related.id"
        );
    }

    #[test]
    fn include_nullable_if_true_left_joins() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_nullable_if(
                true,
                ColumnSelection::new([FakeRelatedColumn::Label]),
                "fake_table.related_id",
                FakeRelatedColumn::Id,
            )
            .build();
        assert_eq!(
            q.sql(),
            "SELECT *, fake_related.id AS fake_related_id, \
             fake_related.label AS fake_related_label \
             FROM fake_table \
             LEFT JOIN fake_related ON fake_table.related_id = fake_related.id"
        );
    }

    #[test]
    fn include_filters_soft_deleted_related_in_on() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include(
                ColumnSelection::new([FakeSoftColumn::Label]),
                "fake_table.soft_id",
                FakeSoftColumn::Id,
            )
            .build();
        // `fake_soft` declares a soft-delete column, so the ON excludes it.
        assert_eq!(
            q.sql(),
            "SELECT *, fake_soft.id AS fake_soft_id, fake_soft.label AS fake_soft_label \
             FROM fake_table \
             INNER JOIN fake_soft ON fake_table.soft_id = fake_soft.id \
             AND fake_soft.deleted_at IS NULL"
        );
    }

    #[test]
    fn include_nullable_filters_soft_deleted_related_in_on() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_nullable(
                ColumnSelection::new([FakeSoftColumn::Label]),
                "fake_table.soft_id",
                FakeSoftColumn::Id,
            )
            .build();
        assert_eq!(
            q.sql(),
            "SELECT *, fake_soft.id AS fake_soft_id, fake_soft.label AS fake_soft_label \
             FROM fake_table \
             LEFT JOIN fake_soft ON fake_table.soft_id = fake_soft.id \
             AND fake_soft.deleted_at IS NULL"
        );
    }

    #[test]
    fn include_with_builds_custom_on_without_soft_delete_filter() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_with(ColumnSelection::new([FakeSoftColumn::Label]), |on| {
                on.eq("fake_table.soft_id", FakeSoftColumn::Id)
            })
            .build();
        // `_with` owns the ON; no automatic `deleted_at IS NULL`.
        assert_eq!(
            q.sql(),
            "SELECT *, fake_soft.id AS fake_soft_id, fake_soft.label AS fake_soft_label \
             FROM fake_table \
             INNER JOIN fake_soft ON fake_table.soft_id = fake_soft.id"
        );
    }

    #[test]
    fn include_nullable_with_uses_left_join_and_custom_on() {
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_nullable_with(ColumnSelection::new([FakeSoftColumn::Label]), |on| {
                on.eq("fake_table.soft_id", FakeSoftColumn::Id)
            })
            .build();
        assert_eq!(
            q.sql(),
            "SELECT *, fake_soft.id AS fake_soft_id, fake_soft.label AS fake_soft_label \
             FROM fake_table \
             LEFT JOIN fake_soft ON fake_table.soft_id = fake_soft.id"
        );
    }

    #[test]
    fn include_nullable_if_false_and_if_some_none_are_noops() {
        let none: Option<ColumnSelection<FakeRelatedColumn>> = None;
        let q = SelectBuilder::new()
            .from("fake_table")
            .all()
            .include_nullable_if(
                false,
                ColumnSelection::new([FakeRelatedColumn::Label]),
                "fake_table.related_id",
                FakeRelatedColumn::Id,
            )
            .include_nullable_if_some(none, "fake_table.related_id", FakeRelatedColumn::Id)
            .build();
        assert_eq!(q.sql(), "SELECT * FROM fake_table");
    }

    #[test]
    #[should_panic(expected = "INNER JOIN requires a non-empty ON predicate")]
    fn inner_join_with_empty_on_panics() {
        let _ = SelectBuilder::new()
            .from("t a")
            .all()
            .inner_join("u b", |on| on)
            .build();
    }

    #[test]
    #[should_panic(expected = "LEFT JOIN requires a non-empty ON predicate")]
    fn left_join_with_empty_on_panics() {
        let _ = SelectBuilder::new()
            .from("t a")
            .all()
            .left_join("u b", |on| on)
            .build();
    }

    #[test]
    #[should_panic(expected = "RIGHT JOIN requires a non-empty ON predicate")]
    fn right_join_with_empty_on_panics() {
        let _ = SelectBuilder::new()
            .from("t a")
            .all()
            .right_join("u b", |on| on)
            .build();
    }

    #[test]
    #[should_panic(expected = "FULL JOIN requires a non-empty ON predicate")]
    fn full_join_with_empty_on_panics() {
        let _ = SelectBuilder::new()
            .from("t a")
            .all()
            .full_join("u b", |on| on)
            .build();
    }

    #[test]
    fn join_on_with_extra_bound_filter() {
        let q = SelectBuilder::new()
            .from("a a")
            .all()
            .inner_join("b b", |on| on.eq("a.id", "b.aid").bound("b.active", true))
            .build();
        assert_eq!(
            q.sql(),
            "SELECT * FROM a a INNER JOIN b b ON a.id = b.aid AND b.active = $1"
        );
    }

    #[test]
    fn join_on_with_or_eq() {
        let q = SelectBuilder::new()
            .from("a a")
            .all()
            .inner_join("b b", |on| {
                on.eq("a.id", "b.aid").or_eq("a.alt_id", "b.aid")
            })
            .build();
        assert_eq!(
            q.sql(),
            "SELECT * FROM a a INNER JOIN b b ON a.id = b.aid OR a.alt_id = b.aid"
        );
    }

    #[test]
    fn join_on_with_or_raw() {
        let q = SelectBuilder::new()
            .from("a a")
            .all()
            .inner_join("b b", |on| on.raw("a.id = b.aid").or_raw("b.active"))
            .build();
        assert_eq!(
            q.sql(),
            "SELECT * FROM a a INNER JOIN b b ON a.id = b.aid OR b.active"
        );
    }

    #[test]
    fn select_distinct() {
        let q = SelectBuilder::new()
            .distinct()
            .from("t")
            .columns(["region"])
            .build();
        assert_eq!(q.sql(), "SELECT DISTINCT region FROM t");
    }

    #[test]
    fn select_distinct_on() {
        let q = SelectBuilder::new()
            .distinct_on(["k"])
            .from("t")
            .all()
            .build();
        assert_eq!(q.sql(), "SELECT DISTINCT ON (k) * FROM t");
    }

    #[test]
    fn select_distinct_on_multiple_columns() {
        let q = SelectBuilder::new()
            .distinct_on(["region", "status"])
            .from("t")
            .all()
            .build();
        assert_eq!(q.sql(), "SELECT DISTINCT ON (region, status) * FROM t");
    }

    #[test]
    fn select_count() {
        let q = SelectBuilder::new().from("t").count().build();
        assert_eq!(q.sql(), "SELECT COUNT(*) FROM t");
    }

    #[test]
    fn select_count_distinct() {
        let q = SelectBuilder::new()
            .from("t")
            .count_distinct("region")
            .build();
        assert_eq!(q.sql(), "SELECT COUNT(DISTINCT region) FROM t");
    }

    #[test]
    fn select_exists_wraps() {
        let q = SelectBuilder::new()
            .from("t")
            .where_eq("id", 1_i32)
            .where_null("deleted_at")
            .exists()
            .build();
        assert_eq!(
            q.sql(),
            "SELECT EXISTS (SELECT 1 FROM t WHERE id = $1 AND deleted_at IS NULL)"
        );
    }

    #[test]
    fn select_with_group_by_and_having() {
        let q = SelectBuilder::new()
            .from("t")
            .expr("region")
            .expr_as("COUNT(*)", "n")
            .where_null("deleted_at")
            .group_by("region")
            .having(|h| h.where_raw("COUNT(*) > 5"))
            .build();
        assert_eq!(
            q.sql(),
            "SELECT region, COUNT(*) AS n FROM t WHERE deleted_at IS NULL \
             GROUP BY region HAVING COUNT(*) > 5"
        );
    }

    #[test]
    fn select_with_order_by_and_limit() {
        let q = SelectBuilder::new()
            .from("t")
            .all()
            .order_by("name", Asc)
            .order_by("id", Desc)
            .limit(20)
            .offset(40)
            .build();
        assert_eq!(
            q.sql(),
            "SELECT * FROM t ORDER BY name ASC, id DESC LIMIT $1 OFFSET $2"
        );
    }

    #[test]
    fn select_with_nulls_order() {
        let q = SelectBuilder::new()
            .from("t")
            .all()
            .order_by_nulls("name", Asc, NullsOrder::Last)
            .build();
        assert_eq!(q.sql(), "SELECT * FROM t ORDER BY name ASC NULLS LAST");
    }

    #[test]
    fn select_union() {
        let a = SelectBuilder::new().from("a").all();
        let b = SelectBuilder::new().from("b").all();
        let q = a.union(b).build();
        assert_eq!(q.sql(), "SELECT * FROM a UNION (SELECT * FROM b)");
    }

    #[test]
    fn select_union_all() {
        let a = SelectBuilder::new().from("a").all();
        let b = SelectBuilder::new().from("b").all();
        let q = a.union_all(b).build();
        assert_eq!(q.sql(), "SELECT * FROM a UNION ALL (SELECT * FROM b)");
    }

    #[test]
    fn select_union_arm_with_order_and_limit_is_parenthesized() {
        let a = SelectBuilder::new().from("a").all();
        let b = SelectBuilder::new()
            .from("b")
            .all()
            .order_by("id", Asc)
            .limit(5);
        let q = a.union_all(b).build();
        assert_eq!(
            q.sql(),
            "SELECT * FROM a UNION ALL (SELECT * FROM b ORDER BY id ASC LIMIT $1)"
        );
    }

    #[test]
    fn select_for_update() {
        let q = SelectBuilder::new()
            .from("t")
            .all()
            .where_eq("id", 1_i32)
            .for_update()
            .build();
        assert_eq!(q.sql(), "SELECT * FROM t WHERE id = $1 FOR UPDATE");
    }

    #[test]
    fn select_for_update_of_skip_locked() {
        let q = SelectBuilder::new()
            .from((FakeTable, "a"))
            .all()
            .for_update()
            .lock_of((FakeTable, "a"))
            .skip_locked()
            .build();
        // `OF` must use the FROM-clause alias; Postgres rejects the
        // underlying table name once the table is aliased.
        assert_eq!(
            q.sql(),
            "SELECT * FROM fake_table a FOR UPDATE OF a SKIP LOCKED"
        );
    }

    #[test]
    fn select_with_cte_raw() {
        let q = SelectBuilder::new()
            .with_cte_raw("recent", |q| {
                q.push("SELECT * FROM t WHERE created_at > ");
                q.push_bind("2024-01-01");
            })
            .from("recent")
            .all()
            .build();
        assert_eq!(
            q.sql(),
            "WITH recent AS (SELECT * FROM t WHERE created_at > $1) SELECT * FROM recent"
        );
    }

    #[test]
    fn select_with_cte_select_subquery() {
        let inner = SelectBuilder::new()
            .from("t")
            .columns(["id", "name"])
            .where_null("deleted_at");
        let q = SelectBuilder::new()
            .with_cte("live_t", inner)
            .from("live_t")
            .all()
            .build();
        assert_eq!(
            q.sql(),
            "WITH live_t AS (SELECT id, name FROM t WHERE deleted_at IS NULL) \
             SELECT * FROM live_t"
        );
    }

    #[test]
    fn expr_with_binds_parameter() {
        let q = SelectBuilder::new()
            .from("t")
            .expr_with_as(
                |q| {
                    q.push("to_tsquery('simple', ");
                    q.push_bind("rust");
                    q.push(")");
                },
                "matches",
            )
            .build();
        assert_eq!(q.sql(), "SELECT to_tsquery('simple', $1) AS matches FROM t");
    }

    #[test]
    fn from_raw_can_be_a_subquery() {
        let q = SelectBuilder::new()
            .from_raw(|q| {
                q.push("(SELECT * FROM t WHERE id < ");
                q.push_bind(100_i32);
                q.push(") sub");
            })
            .all()
            .build();
        assert_eq!(q.sql(), "SELECT * FROM (SELECT * FROM t WHERE id < $1) sub");
    }
}

#[cfg(all(test, feature = "cursor-pagination"))]
mod cursor_paginate_tests {
    use crate::ordering::{OrderBy, OrderByDirection};
    use crate::pagination::cursor::{CursorPagination, CursorPayload, CursorValue};
    use crate::query_builder::{Query, WhereOps};
    use crate::test_support::{CountriesColumn, CountriesTable};

    #[test]
    fn select_cursor_paginate_no_cursor_emits_order_and_limit() {
        let pagination =
            CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
                OrderBy::new(CountriesColumn::Name, OrderByDirection::Asc),
                20,
                None,
                None,
            )
            .unwrap();

        let q = Query::select()
            .from(CountriesTable)
            .all()
            .where_null(CountriesColumn::Name)
            .cursor_paginate(&pagination)
            .build();

        assert_eq!(
            q.sql(),
            "SELECT * FROM geo_countries WHERE geo_countries.name IS NULL \
             ORDER BY geo_countries.name ASC NULLS LAST, \
             geo_countries.id ASC NULLS LAST LIMIT $1"
        );
    }

    #[test]
    fn select_cursor_paginate_with_cursor_asc() {
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

        let q = Query::select()
            .from(CountriesTable)
            .all()
            .where_null("deleted_at")
            .cursor_paginate(&pagination)
            .build();

        assert_eq!(
            q.sql(),
            "SELECT * FROM geo_countries WHERE deleted_at IS NULL AND \
             (geo_countries.name > $1 OR (geo_countries.name = $2 AND geo_countries.id > $3) OR geo_countries.name IS NULL) \
             ORDER BY geo_countries.name ASC NULLS LAST, geo_countries.id ASC NULLS LAST LIMIT $4"
        );
    }

    #[test]
    fn select_cursor_paginate_with_cursor_asc_null_val() {
        let cursor = CursorPayload::with_filter_sig(
            CountriesColumn::Name,
            OrderByDirection::Asc,
            42_i32,
            None,
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

        let q = Query::select()
            .from(CountriesTable)
            .all()
            .cursor_paginate(&pagination)
            .build();

        assert_eq!(
            q.sql(),
            "SELECT * FROM geo_countries WHERE \
             (geo_countries.name IS NULL AND geo_countries.id > $1) \
             ORDER BY geo_countries.name ASC NULLS LAST, geo_countries.id ASC NULLS LAST LIMIT $2"
        );
    }

    #[test]
    fn select_cursor_paginate_with_cursor_desc() {
        let cursor = CursorPayload::with_filter_sig(
            CountriesColumn::CreatedAt,
            OrderByDirection::Desc,
            7_i32,
            Some(CursorValue::Text("2024-01-01".to_string())),
            None,
        )
        .encode()
        .unwrap();
        let pagination =
            CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
                OrderBy::new(CountriesColumn::CreatedAt, OrderByDirection::Desc),
                10,
                Some(cursor),
                None,
            )
            .unwrap();

        let q = Query::select()
            .from(CountriesTable)
            .all()
            .cursor_paginate(&pagination)
            .build();

        assert_eq!(
            q.sql(),
            "SELECT * FROM geo_countries WHERE \
             (geo_countries.created_at < $1 OR (geo_countries.created_at = $2 AND geo_countries.id < $3)) \
             ORDER BY geo_countries.created_at DESC NULLS FIRST, geo_countries.id DESC NULLS FIRST LIMIT $4"
        );
    }

    #[test]
    fn select_cursor_paginate_with_cursor_desc_null_val() {
        let cursor = CursorPayload::with_filter_sig(
            CountriesColumn::Name,
            OrderByDirection::Desc,
            42_i32,
            None,
            None,
        )
        .encode()
        .unwrap();
        let pagination =
            CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
                OrderBy::new(CountriesColumn::Name, OrderByDirection::Desc),
                20,
                Some(cursor),
                None,
            )
            .unwrap();

        let q = Query::select()
            .from(CountriesTable)
            .all()
            .cursor_paginate(&pagination)
            .build();

        assert_eq!(
            q.sql(),
            "SELECT * FROM geo_countries WHERE \
             ((geo_countries.name IS NULL AND geo_countries.id < $1) OR geo_countries.name IS NOT NULL) \
             ORDER BY geo_countries.name DESC NULLS FIRST, geo_countries.id DESC NULLS FIRST LIMIT $2"
        );
    }

    #[test]
    fn cursor_paginate_with_uuid_id() {
        use uuid::Uuid;

        let id = Uuid::parse_str("018e3c8e-8b2a-7b4b-bb2a-1d2e3c4b5a6f").unwrap();
        let cursor = CursorPayload::with_filter_sig(
            CountriesColumn::Name,
            OrderByDirection::Asc,
            id,
            Some(CursorValue::Text("alice".to_string())),
            None,
        )
        .encode()
        .unwrap();
        let pagination =
            CursorPagination::<CountriesColumn, Uuid>::with_validated_column_selection::<()>(
                OrderBy::new(CountriesColumn::Name, OrderByDirection::Asc),
                20,
                Some(cursor),
                None,
            )
            .unwrap();

        let q = Query::select()
            .from(CountriesTable)
            .all()
            .cursor_paginate(&pagination)
            .build();

        // SQL pattern: $1=val, $2=val (clone), $3=id (uuid), $4=limit
        let sql = q.sql();
        assert!(sql.as_str().contains("geo_countries.id > $3"));
        assert!(sql.as_str().contains("LIMIT $4"));
    }
}
