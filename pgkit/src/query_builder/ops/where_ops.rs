use sqlx::postgres::PgHasArrayType;
use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::has_where::HasWhere;
use crate::query_builder::operators::ComparisonOperator;
use crate::query_builder::where_builder::WhereBuilder;
use crate::types::ColumnRef;

/// Generate `(column, operator, value)` delegations.
macro_rules! delegate_col_op_value {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<C, T>(self, column: C, operator: ComparisonOperator, value: T) -> Self
        where
            C: Into<ColumnRef>,
            T: Encode<$lt, Postgres> + Type<Postgres> + Send + $lt,
        {
            self.map_where(|w| w.$name(column, operator, value))
        }
    )+};
}

/// Generate `(column, value)` delegations.
macro_rules! delegate_col_value {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<C, T>(self, column: C, value: T) -> Self
        where
            C: Into<ColumnRef>,
            T: Encode<$lt, Postgres> + Type<Postgres> + Send + $lt,
        {
            self.map_where(|w| w.$name(column, value))
        }
    )+};
}

/// Generate `(column)` delegations (null checks).
macro_rules! delegate_col {
    ($($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<C: Into<ColumnRef>>(self, column: C) -> Self {
            self.map_where(|w| w.$name(column))
        }
    )+};
}

/// Generate `(column, Vec<value>)` delegations (expanded IN-lists).
macro_rules! delegate_col_vec {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<C, T>(self, column: C, value: Vec<T>) -> Self
        where
            C: Into<ColumnRef>,
            T: Encode<$lt, Postgres> + Type<Postgres> + Send + $lt,
        {
            self.map_where(|w| w.$name(column, value))
        }
    )+};
}

/// Generate `(column, Vec<value>)` delegations binding a single array parameter.
macro_rules! delegate_col_vec_array {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<C, T>(self, column: C, value: Vec<T>) -> Self
        where
            C: Into<ColumnRef>,
            T: Encode<$lt, Postgres> + Type<Postgres> + PgHasArrayType + Send + $lt,
        {
            self.map_where(|w| w.$name(column, value))
        }
    )+};
}

/// Generate `(column, operator, Vec<value>)` delegations binding a single array parameter.
macro_rules! delegate_col_op_vec_array {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<C, T>(self, column: C, operator: ComparisonOperator, value: Vec<T>) -> Self
        where
            C: Into<ColumnRef>,
            T: Encode<$lt, Postgres> + Type<Postgres> + PgHasArrayType + Send + $lt,
        {
            self.map_where(|w| w.$name(column, operator, value))
        }
    )+};
}

/// Generate `(column, lower, upper)` delegations (BETWEEN family).
macro_rules! delegate_between {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<C, T>(self, column: C, lower: T, upper: T) -> Self
        where
            C: Into<ColumnRef>,
            T: Encode<$lt, Postgres> + Type<Postgres> + Send + $lt,
        {
            self.map_where(|w| w.$name(column, lower, upper))
        }
    )+};
}

/// Generate `(&str)` delegations (raw fragments and static EXISTS bodies).
macro_rules! delegate_raw_sql {
    ($($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and the injection warning.")]
        fn $name(self, sql: &str) -> Self {
            let owned = sql.to_string();
            self.map_where(move |w| w.$name(&owned))
        }
    )+};
}

/// Generate deferred-closure delegations (parameter-binding raw fragments and EXISTS bodies).
macro_rules! delegate_raw_with {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<F>(self, callback: F) -> Self
        where
            F: FnOnce(&mut QueryBuilder<Postgres>) + Send + $lt,
        {
            self.map_where(|w| w.$name(callback))
        }
    )+};
}

/// Generate `(Option<value>, closure)` delegations.
macro_rules! delegate_raw_with_if_some {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<T, F>(self, value: Option<T>, callback: F) -> Self
        where
            T: Send + $lt,
            F: FnOnce(T, &mut QueryBuilder<Postgres>) + Send + $lt,
        {
            self.map_where(|w| w.$name(value, callback))
        }
    )+};
}

/// Generate grouping delegations (closure over a fresh inner [`WhereBuilder`]).
macro_rules! delegate_group {
    ($lt:lifetime; $($name:ident),+ $(,)?) => {$(
        #[doc = concat!("Delegates to [`WhereBuilder::", stringify!($name), "`]; see it for SQL semantics and examples.")]
        fn $name<F>(self, callback: F) -> Self
        where
            F: FnOnce(WhereBuilder<$lt>) -> WhereBuilder<$lt>,
        {
            self.map_where(|w| w.$name(callback))
        }
    )+};
}

/// WHERE-clause surface, mirrored from [`WhereBuilder`] onto any builder that
/// implements [`HasWhere`].
///
/// Every method is a one-line delegation to the same-named method on the
/// embedded [`WhereBuilder`], generated by the `delegate_*` macros above,
/// one macro per parameter shape. See [`WhereBuilder`] for the canonical
/// descriptions and SQL semantics.
///
/// # Example
/// ```
/// # use pgkit::query_builder::{SelectBuilder, WhereBuilder, WhereOps};
/// // SelectBuilder implements HasWhere, so it picks up the entire WhereOps
/// // surface via the blanket impl.
/// let q = SelectBuilder::new()
///     .from("users")
///     .where_eq("status", "active")
///     .where_gt("age", 18)
///     .or_group(|g| g.where_null("deleted_at").where_eq("is_admin", true))
///     .build();
/// assert_eq!(
///     q.sql(),
///     "SELECT * FROM users WHERE ((status = $1 AND age > $2) OR (deleted_at IS NULL AND is_admin = $3))"
/// );
/// ```
pub trait WhereOps<'a>: HasWhere<'a> + Sized {
    /// Apply a function to the embedded WHERE-clause.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::{SelectBuilder, WhereBuilder, WhereOps};
    /// // Apply a reusable filter helper to the builder's WHERE clause.
    /// fn not_deleted<'a>(w: WhereBuilder<'a>) -> WhereBuilder<'a> {
    ///     w.where_null("deleted_at")
    /// }
    ///
    /// let q = SelectBuilder::new()
    ///     .from("users")
    ///     .map_where(not_deleted)
    ///     .build();
    /// assert_eq!(q.sql(), "SELECT * FROM users WHERE deleted_at IS NULL");
    /// ```
    fn map_where<F>(mut self, f: F) -> Self
    where
        F: FnOnce(WhereBuilder<'a>) -> WhereBuilder<'a>,
    {
        let w = std::mem::take(self.where_mut());
        *self.where_mut() = f(w);
        self
    }

    delegate_col_op_value!('a; where_col, or_where_col);

    delegate_col_value!('a;
        where_eq, where_ne, where_gt, where_lt, where_gte, where_lte,
        or_where_eq, or_where_ne, or_where_gt, or_where_lt, or_where_gte, or_where_lte,
    );

    delegate_col!(where_null, or_where_null, where_not_null, or_where_not_null);

    delegate_col_vec!('a; where_in, or_where_in, where_not_in, or_where_not_in);

    delegate_col_vec_array!('a; where_any, or_where_any);

    delegate_col_op_vec_array!('a; where_any_op, or_where_any_op, where_all, or_where_all);

    delegate_between!('a; where_between, or_where_between, where_not_between, or_where_not_between);

    /// Adds a basic WHERE condition only if `value` is `Some`. Uses `AND`.
    /// See [`WhereBuilder::where_col_if_some`].
    fn where_col_if_some<C, T>(
        self,
        column: C,
        operator: ComparisonOperator,
        value: Option<T>,
    ) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.map_where(|w| w.where_col_if_some(column, operator, value))
    }

    /// Apply `callback` to **this builder** (not just its WHERE clause) when
    /// `condition` is true. Useful for conditionally adding any builder method,
    /// not only WHERE predicates, e.g. `b.when(opts.with_join, |b| b.inner_join(...))`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::{SelectBuilder, WhereBuilder, WhereOps};
    /// // Conditionally include archived rows.
    /// let include_archived = true;
    /// let q = SelectBuilder::new()
    ///     .from("users")
    ///     .where_eq("active", true)
    ///     .when(include_archived, |b| b.or_where_not_null("archived_at"))
    ///     .build();
    /// assert_eq!(q.sql(), "SELECT * FROM users WHERE active = $1 OR archived_at IS NOT NULL");
    /// ```
    fn when<F>(self, condition: bool, callback: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        if condition { callback(self) } else { self }
    }

    delegate_group!('a; group, or_group);

    delegate_raw_sql!(
        where_raw,
        or_where_raw,
        where_exists,
        or_where_exists,
        where_not_exists,
        or_where_not_exists,
    );

    delegate_raw_with!('a;
        where_raw_with, or_where_raw_with,
        where_exists_with, or_where_exists_with,
        where_not_exists_with, or_where_not_exists_with,
    );

    delegate_raw_with_if_some!('a; where_raw_with_if_some, or_where_raw_with_if_some);

    /// Replace the embedded WHERE-clause wholesale. Useful when filters are
    /// computed elsewhere and handed in pre-built, e.g. shared between a
    /// COUNT and a paged SELECT, or built by a helper that returns a
    /// [`WhereBuilder`].
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::{SelectBuilder, WhereBuilder, WhereOps};
    /// fn visibility_filter<'a>(user_id: i64) -> WhereBuilder<'a> {
    ///     WhereBuilder::new()
    ///         .where_null("deleted_at")
    ///         .where_eq("owner_id", user_id)
    /// }
    ///
    /// let user_id = 42;
    /// let q = SelectBuilder::new()
    ///     .from("documents")
    ///     .where_with(visibility_filter(user_id))
    ///     .build();
    /// assert_eq!(q.sql(), "SELECT * FROM documents WHERE deleted_at IS NULL AND owner_id = $1");
    /// ```
    fn where_with(mut self, other: WhereBuilder<'a>) -> Self {
        *self.where_mut() = other;
        self
    }
}

impl<'a, T: HasWhere<'a>> WhereOps<'a> for T {}
