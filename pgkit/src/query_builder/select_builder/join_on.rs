use sqlx::{Encode, Postgres, QueryBuilder, Type};

use crate::query_builder::where_builder::WhereBuilder;
use crate::types::ColumnRef;

/// Builder for the `ON …` predicate of a JOIN.
///
/// Reuses [`WhereBuilder`]'s machinery; call [`eq`](Self::eq) for the common
/// `a.x = b.x` equi-join (chain more calls to AND them), [`or_eq`](Self::or_eq)
/// to OR another, or [`bound`](Self::bound) / [`raw`](Self::raw) for filters
/// baked into the JOIN rather than the WHERE.
///
/// ```
/// # use pgkit::query_builder::Query;
/// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
/// let query = Query::select()
///     .from("addresses a")
///     .columns(["a.id"])
///     .inner_join((CountriesTable, "c"), |on| on
///         .eq("a.country_id", "c.id")
///         .bound("c.active", true))
///     .build();
/// assert_eq!(
///     query.sql(),
///     "SELECT a.id FROM addresses a INNER JOIN geo_countries c ON a.country_id = c.id AND c.active = $1"
/// );
/// ```
pub struct JoinOn<'a> {
    pub(super) inner: WhereBuilder<'a>,
}

impl<'a> JoinOn<'a> {
    pub(crate) fn new() -> Self {
        Self {
            inner: WhereBuilder::new(),
        }
    }

    /// `left = right`: both column references. The most common JOIN ON shape.
    pub fn eq<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<ColumnRef>,
        R: Into<ColumnRef>,
    {
        let left = left.into();
        let right = right.into();
        self.with_inner(
            move |q| {
                q.push(&left);
                q.push(" = ");
                q.push(&right);
            },
            false,
        )
    }

    /// `OR left = right`.
    pub fn or_eq<L, R>(self, left: L, right: R) -> Self
    where
        L: Into<ColumnRef>,
        R: Into<ColumnRef>,
    {
        let left = left.into();
        let right = right.into();
        self.with_inner(
            move |q| {
                q.push(&left);
                q.push(" = ");
                q.push(&right);
            },
            true,
        )
    }

    /// `col IS NULL`: added with `AND`.
    pub fn null<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        self.inner = self.inner.where_null(column);
        self
    }

    /// `col = $N`: bind a literal value into the JOIN predicate.
    pub fn bound<C, T>(mut self, column: C, value: T) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.inner = self.inner.where_eq(column, value);
        self
    }

    /// `OR col = $N`.
    pub fn or_bound<C, T>(mut self, column: C, value: T) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.inner = self.inner.or_where_eq(column, value);
        self
    }

    /// Raw SQL fragment as the predicate.
    pub fn raw(self, sql: impl Into<String>) -> Self {
        self.raw_inner(sql.into(), false)
    }

    /// `OR <sql>`.
    pub fn or_raw(self, sql: impl Into<String>) -> Self {
        self.raw_inner(sql.into(), true)
    }

    /// Deferred fragment with parameter bindings.
    pub fn raw_with<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.inner = self.inner.where_raw_with(callback);
        self
    }

    fn raw_inner(mut self, sql: String, or_join: bool) -> Self {
        if or_join {
            self.inner = self.inner.or_where_raw(&sql);
        } else {
            self.inner = self.inner.where_raw(&sql);
        }
        self
    }

    fn with_inner<F>(mut self, callback: F, or_join: bool) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        if or_join {
            self.inner = self.inner.or_where_raw_with(callback);
        } else {
            self.inner = self.inner.where_raw_with(callback);
        }
        self
    }

    pub(super) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<'a> Default for JoinOn<'a> {
    fn default() -> Self {
        Self::new()
    }
}
