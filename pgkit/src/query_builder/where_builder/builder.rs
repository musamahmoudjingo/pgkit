use sqlx::postgres::PgHasArrayType;
use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::super::operators::*;
use super::some_where_condition::apply_conditions;
use super::{
    SomeWhereCondition, WhereAll, WhereAny, WhereBetween, WhereCondition, WhereExists, WhereGroup,
    WhereIn, WhereNotBetween, WhereNotExists, WhereNotIn, WhereNotNull, WhereNull, WhereRaw,
    WhereRawFn,
};
use crate::types::ColumnRef;

/// A builder for constructing SQL WHERE clauses dynamically for PostgreSQL.
///
/// This builder provides a fluent interface to add various conditions (equality,
/// comparison, null checks, IN, BETWEEN, grouping, etc.) with appropriate `AND` or `OR`
/// separators. It handles parameter binding using `sqlx`.
///
/// Conditions chain left-associatively: when a clause mixes `AND` and `OR`,
/// the rendered SQL is parenthesized so it evaluates in call order:
/// `.where_eq(a).or_where_eq(b).where_gt(c)` means `(a OR b) AND c`, not the
/// `a OR (b AND c)` that flat SQL precedence would produce.
///
/// # Example
/// ```
/// # use pgkit::query_builder::WhereBuilder;
/// # use pgkit::query_builder::operators::*;
/// let mut query = sqlx::QueryBuilder::new("SELECT * FROM users");
/// let applied = WhereBuilder::new()
///     .where_eq("status", "active")
///     .where_gt("age", 18)
///     .or_group(|g| g.where_null("deleted_at").where_eq("is_admin", true))
///     .apply_to(&mut query);
/// assert!(applied);
/// assert_eq!(
///     query.sql(),
///     "SELECT * FROM users WHERE ((status = $1 AND age > $2) OR (deleted_at IS NULL AND is_admin = $3))"
/// );
/// ```
#[derive(Default)]
pub struct WhereBuilder<'a> {
    conditions: Vec<Box<dyn SomeWhereCondition<'a> + 'a + Send>>,
    has_and: bool,
    has_or: bool,
}

impl<'a> WhereBuilder<'a> {
    /// Creates a new, empty `WhereBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Determines the appropriate logical operator (`AND`) if conditions already exist.
    fn and(&mut self) -> Option<LogicalOperator> {
        if self.conditions.is_empty() {
            None
        } else {
            self.has_and = true;
            Some(And)
        }
    }

    /// Determines the appropriate logical operator (`OR`) if conditions already exist.
    fn or(&mut self) -> Option<LogicalOperator> {
        if self.conditions.is_empty() {
            None
        } else {
            self.has_or = true;
            Some(Or)
        }
    }

    /// True when `AND` and `OR` are mixed at this level; flat SQL precedence
    /// would then diverge from call order, so rendering must parenthesize.
    fn mixed(&self) -> bool {
        self.has_and && self.has_or
    }

    // --- Core Where Methods ---

    /// Adds a basic WHERE condition with an `AND` separator if needed.
    ///
    /// # Arguments
    /// * `column` - The name of the column.
    /// * `operator` - The comparison operator (e.g., `=`, `>`, `LIKE`).
    /// * `value` - The value to bind for the comparison.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_col("name", Equal, "rustacean")
    ///     .where_col("age", GreaterThan, 16)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE name = $1 AND age > $2");
    /// ```
    pub fn where_col<C: Into<ColumnRef>, T>(
        mut self,
        column: C,
        operator: ComparisonOperator,
        value: T,
    ) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereCondition {
            column: column.into(),
            operator,
            value,
            separator,
        }));

        self
    }

    /// Adds a basic WHERE condition with an `OR` separator if needed.
    ///
    /// # Arguments
    /// * `column` - The name of the column.
    /// * `operator` - The comparison operator (e.g., `=`, `>`, `LIKE`).
    /// * `value` - The value to bind for the comparison.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_col("status", Equal, "active")
    ///     .or_where_col("role", Equal, "admin")
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1 OR role = $2");
    /// ```
    pub fn or_where_col<C: Into<ColumnRef>, T>(
        mut self,
        column: C,
        operator: ComparisonOperator,
        value: T,
    ) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereCondition {
            column: column.into(),
            operator,
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column = value` condition using `AND`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new().where_eq("status", "active").apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1");
    /// ```
    pub fn where_eq<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.where_col(column, Equal, value)
    }

    /// Adds a `WHERE column <> value` condition using `AND`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new().where_ne("status", "inactive").apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status <> $1");
    /// ```
    pub fn where_ne<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.where_col(column, NotEqual, value)
    }

    /// Adds a `WHERE column > value` condition using `AND`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new().where_gt("score", 90).apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE score > $1");
    /// ```
    pub fn where_gt<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.where_col(column, GreaterThan, value)
    }

    /// Adds a `WHERE column < value` condition using `AND`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new().where_lt("attempts", 3).apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE attempts < $1");
    /// ```
    pub fn where_lt<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.where_col(column, LessThan, value)
    }

    /// Adds a `WHERE column >= value` condition using `AND`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new().where_gte("age", 18).apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE age >= $1");
    /// ```
    pub fn where_gte<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.where_col(column, GreaterThanOrEqual, value)
    }

    /// Adds a `WHERE column <= value` condition using `AND`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new().where_lte("price", 100.0).apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE price <= $1");
    /// ```
    pub fn where_lte<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.where_col(column, LessThanOrEqual, value)
    }

    /// Adds a `WHERE column = value` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("status", "active")
    ///     .or_where_eq("role", "admin")
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1 OR role = $2");
    /// ```
    pub fn or_where_eq<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.or_where_col(column, Equal, value)
    }

    /// Adds a `WHERE column <> value` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("status", "active")
    ///     .or_where_ne("plan", "free")
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1 OR plan <> $2");
    /// ```
    pub fn or_where_ne<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.or_where_col(column, NotEqual, value)
    }

    /// Adds a `WHERE column > value` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("featured", true)
    ///     .or_where_gt("score", 90)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE featured = $1 OR score > $2");
    /// ```
    pub fn or_where_gt<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.or_where_col(column, GreaterThan, value)
    }

    /// Adds a `WHERE column < value` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("archived", false)
    ///     .or_where_lt("attempts", 3)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE archived = $1 OR attempts < $2");
    /// ```
    pub fn or_where_lt<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.or_where_col(column, LessThan, value)
    }

    /// Adds a `WHERE column >= value` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("is_admin", true)
    ///     .or_where_gte("age", 18)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE is_admin = $1 OR age >= $2");
    /// ```
    pub fn or_where_gte<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.or_where_col(column, GreaterThanOrEqual, value)
    }

    /// Adds a `WHERE column <= value` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("is_sale", true)
    ///     .or_where_lte("price", 100.0)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE is_sale = $1 OR price <= $2");
    /// ```
    pub fn or_where_lte<C: Into<ColumnRef>, T>(self, column: C, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.or_where_col(column, LessThanOrEqual, value)
    }

    /// Adds a `WHERE column IS NULL` condition using `AND`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new().where_null("deleted_at").apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE deleted_at IS NULL");
    /// ```
    pub fn where_null<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        let separator = self.and();
        self.conditions.push(Box::new(WhereNull {
            column: column.into(),
            separator,
        }));

        self
    }

    /// Adds a `WHERE column IS NULL` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("status", "archived")
    ///     .or_where_null("deleted_at")
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1 OR deleted_at IS NULL");
    /// ```
    pub fn or_where_null<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        let separator = self.or();
        self.conditions.push(Box::new(WhereNull {
            column: column.into(),
            separator,
        }));

        self
    }

    /// Adds a `WHERE column IS NOT NULL` condition using `AND`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new().where_not_null("deleted_at").apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE deleted_at IS NOT NULL");
    /// ```
    pub fn where_not_null<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        let separator = self.and();
        self.conditions.push(Box::new(WhereNotNull {
            column: column.into(),
            separator,
        }));

        self
    }

    /// Adds a `WHERE column IS NOT NULL` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("status", "active")
    ///     .or_where_not_null("archived_at")
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1 OR archived_at IS NOT NULL");
    /// ```
    pub fn or_where_not_null<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        let separator = self.or();
        self.conditions.push(Box::new(WhereNotNull {
            column: column.into(),
            separator,
        }));

        self
    }

    /// Adds a `WHERE column IN (...)` condition using `AND`.
    ///
    /// Each element in `value` will be bound as a separate parameter.
    /// An empty `value` vector emits `FALSE`: membership in an empty set
    /// matches nothing, mirroring SQL semantics.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_in("status", vec!["active", "pending"])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status IN ($1, $2)");
    /// ```
    pub fn where_in<C: Into<ColumnRef>, T>(mut self, column: C, value: Vec<T>) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereIn {
            column: column.into(),
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column IN (...)` condition using `OR`.
    ///
    /// Each element in `value` will be bound as a separate parameter.
    /// An empty `value` vector emits `FALSE`: membership in an empty set
    /// matches nothing, mirroring SQL semantics.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .or_where_in("status", vec!["pending", "review"])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE active = $1 OR status IN ($2, $3)");
    /// ```
    pub fn or_where_in<C: Into<ColumnRef>, T>(mut self, column: C, value: Vec<T>) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereIn {
            column: column.into(),
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column NOT IN (...)` condition using `AND`.
    ///
    /// Each element in `value` will be bound as a separate parameter.
    /// An empty `value` vector emits `TRUE`: exclusion from an empty set
    /// matches everything, mirroring SQL semantics.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_not_in("status", vec!["inactive", "banned"])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status NOT IN ($1, $2)");
    /// ```
    pub fn where_not_in<C: Into<ColumnRef>, T>(mut self, column: C, value: Vec<T>) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereNotIn {
            column: column.into(),
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column NOT IN (...)` condition using `OR`.
    ///
    /// Each element in `value` will be bound as a separate parameter.
    /// An empty `value` vector emits `TRUE`: exclusion from an empty set
    /// matches everything, mirroring SQL semantics.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .or_where_not_in("status", vec!["banned", "deleted"])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE active = $1 OR status NOT IN ($2, $3)");
    /// ```
    pub fn or_where_not_in<C: Into<ColumnRef>, T>(mut self, column: C, value: Vec<T>) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereNotIn {
            column: column.into(),
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column = ANY($1)` condition using `AND`.
    ///
    /// Binds the entire vector as a single PostgreSQL array parameter (rather than
    /// inlining each value as `WHERE column IN ($1, $2, ...)`). Functionally
    /// equivalent to `where_in` when used with `=`, but more efficient for large
    /// arrays and works with any comparison operator via [`where_any_op`].
    ///
    /// An empty `value` vector binds an empty array, which matches nothing
    /// (`= ANY('{}')` is `FALSE`), exact SQL semantics.
    ///
    /// [`where_any_op`]: Self::where_any_op
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_any("status", vec!["active", "pending"])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = ANY($1)");
    /// ```
    pub fn where_any<C: Into<ColumnRef>, T>(self, column: C, value: Vec<T>) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + PgHasArrayType + Send + 'a,
    {
        self.where_any_op(column, Equal, value)
    }

    /// Adds a `WHERE column = ANY($1)` condition using `OR`.
    ///
    /// See [`where_any`](Self::where_any).
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .or_where_any("status", vec!["pending"])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE active = $1 OR status = ANY($2)");
    /// ```
    pub fn or_where_any<C: Into<ColumnRef>, T>(self, column: C, value: Vec<T>) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + PgHasArrayType + Send + 'a,
    {
        self.or_where_any_op(column, Equal, value)
    }

    /// Adds a `WHERE column op ANY($1)` condition using `AND`.
    ///
    /// Like [`where_any`](Self::where_any) but accepts an arbitrary comparison
    /// operator, e.g. `column <> ANY(arr)`, `column LIKE ANY(arr)`,
    /// `column > ANY(arr)`.
    ///
    /// An empty `value` vector binds an empty array, which matches nothing
    /// (`op ANY('{}')` is `FALSE`), exact SQL semantics.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_any_op("score", GreaterThan, vec![10, 20, 30])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE score > ANY($1)");
    /// ```
    pub fn where_any_op<C: Into<ColumnRef>, T>(
        mut self,
        column: C,
        operator: ComparisonOperator,
        value: Vec<T>,
    ) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + PgHasArrayType + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereAny {
            column: column.into(),
            operator,
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column op ANY($1)` condition using `OR`.
    ///
    /// See [`where_any_op`](Self::where_any_op).
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .or_where_any_op("score", GreaterThan, vec![10, 20])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE active = $1 OR score > ANY($2)");
    /// ```
    pub fn or_where_any_op<C: Into<ColumnRef>, T>(
        mut self,
        column: C,
        operator: ComparisonOperator,
        value: Vec<T>,
    ) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + PgHasArrayType + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereAny {
            column: column.into(),
            operator,
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column op ALL($1)` condition using `AND`.
    ///
    /// The condition holds when the operator is satisfied against *every*
    /// element of the array, e.g. `column > ALL(arr)` (strictly greater than
    /// every element), `column <> ALL(arr)` (equivalent to `NOT IN`).
    ///
    /// An empty `value` vector binds an empty array, which matches everything
    /// (`op ALL('{}')` is `TRUE`), exact SQL semantics.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_all("score", GreaterThan, vec![10, 20, 30])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE score > ALL($1)");
    /// ```
    pub fn where_all<C: Into<ColumnRef>, T>(
        mut self,
        column: C,
        operator: ComparisonOperator,
        value: Vec<T>,
    ) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + PgHasArrayType + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereAll {
            column: column.into(),
            operator,
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column op ALL($1)` condition using `OR`.
    ///
    /// See [`where_all`](Self::where_all).
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .or_where_all("score", NotEqual, vec![0])
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE active = $1 OR score <> ALL($2)");
    /// ```
    pub fn or_where_all<C: Into<ColumnRef>, T>(
        mut self,
        column: C,
        operator: ComparisonOperator,
        value: Vec<T>,
    ) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + PgHasArrayType + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereAll {
            column: column.into(),
            operator,
            value,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column BETWEEN lower AND upper` condition using `AND`.
    ///
    /// Inclusive on both ends, equivalent to `column >= lower AND column <= upper`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_between("age", 18, 30)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE age BETWEEN $1 AND $2");
    /// ```
    pub fn where_between<C: Into<ColumnRef>, T>(mut self, column: C, lower: T, upper: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereBetween {
            column: column.into(),
            lower,
            upper,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column BETWEEN lower AND upper` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .or_where_between("age", 18, 30)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE active = $1 OR age BETWEEN $2 AND $3");
    /// ```
    pub fn or_where_between<C: Into<ColumnRef>, T>(mut self, column: C, lower: T, upper: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereBetween {
            column: column.into(),
            lower,
            upper,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column NOT BETWEEN lower AND upper` condition using `AND`.
    ///
    /// Equivalent to `column < lower OR column > upper`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_not_between("age", 18, 30)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE age NOT BETWEEN $1 AND $2");
    /// ```
    pub fn where_not_between<C: Into<ColumnRef>, T>(mut self, column: C, lower: T, upper: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereNotBetween {
            column: column.into(),
            lower,
            upper,
            separator,
        }));

        self
    }

    /// Adds a `WHERE column NOT BETWEEN lower AND upper` condition using `OR`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .or_where_not_between("age", 18, 30)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE active = $1 OR age NOT BETWEEN $2 AND $3");
    /// ```
    pub fn or_where_not_between<C: Into<ColumnRef>, T>(
        mut self,
        column: C,
        lower: T,
        upper: T,
    ) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereNotBetween {
            column: column.into(),
            lower,
            upper,
            separator,
        }));

        self
    }

    // --- Conditional Adding ---

    /// Adds a basic WHERE condition only if `value` is `Some`.
    /// Uses `AND` as the separator if needed.
    ///
    /// # Arguments
    /// * `column` - The name of the column.
    /// * `operator` - The comparison operator.
    /// * `value` - An `Option<T>` containing the value to bind if `Some`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// // With None: no condition added, no SQL emitted.
    /// WhereBuilder::new()
    ///     .where_col_if_some("status", Equal, None::<&str>)
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), "");
    ///
    /// // With Some: the condition is added.
    /// WhereBuilder::new()
    ///     .where_col_if_some("status", Equal, Some("active"))
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1");
    /// ```
    pub fn where_col_if_some<C: Into<ColumnRef>, T>(
        self,
        column: C,
        operator: ComparisonOperator,
        value: Option<T>,
    ) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        if value.is_some() {
            self.where_col(column, operator, value)
        } else {
            self
        }
    }

    /// Adds a WHERE condition using a closure only if the condition is true.
    /// The closure receives the current `WhereBuilder` to add conditions to.
    /// Conditions added within the closure will be connected using `AND` or `OR` based on
    /// the context where `when` is called.
    ///
    /// # Arguments
    /// * `condition` - A boolean indicating whether to execute the callback.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// // With `include_deleted = false` this would be just " WHERE status = $1".
    /// let include_deleted = true;
    /// WhereBuilder::new()
    ///     .where_eq("status", "archived")
    ///     .when(include_deleted, |b| b.where_not_null("deleted_at"))
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1 AND deleted_at IS NOT NULL");
    /// ```
    pub fn when<F>(self, condition: bool, callback: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        if condition { callback(self) } else { self }
    }

    // -- Grouping

    /// Adds a group of conditions enclosed in parentheses, connected by `AND`.
    /// The closure receives a fresh `WhereBuilder` instance to define the conditions within the group.
    /// If the inner builder ends up empty, no group is emitted.
    ///
    /// # Arguments
    /// * `callback` - A closure that takes a `WhereBuilder` and returns it after adding conditions.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("department", "software")
    ///     .group(|b| b.where_eq("name", "rustacean").where_gt("age", 16))
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE department = $1 AND (name = $2 AND age > $3)");
    /// ```
    pub fn group<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        let builder = callback(Self::new());
        if !builder.conditions.is_empty() {
            let separator = self.and();
            self.conditions.push(Box::new(WhereGroup {
                mixed: builder.mixed(),
                conditions: builder.conditions,
                separator,
            }));
        }

        self
    }

    /// Adds a group of conditions enclosed in parentheses, connected by `OR`.
    /// The closure receives a fresh `WhereBuilder` instance to define the conditions within the group.
    /// If the inner builder ends up empty, no group is emitted.
    ///
    /// # Arguments
    /// * `callback` - A closure that takes a `WhereBuilder` and returns it after adding conditions.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("a", 1)
    ///     .or_group(|b| b.where_eq("b", 2).where_eq("c", 3))
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE a = $1 OR (b = $2 AND c = $3)");
    /// ```
    pub fn or_group<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        let builder = callback(Self::new());
        if !builder.conditions.is_empty() {
            let separator = self.or();
            self.conditions.push(Box::new(WhereGroup {
                mixed: builder.mixed(),
                conditions: builder.conditions,
                separator,
            }));
        }

        self
    }

    // --- Raw SQL ---

    /// Adds a raw SQL fragment as a WHERE condition using `AND`.
    ///
    /// Use this for static SQL fragments that don't require parameter binding,
    /// such as EXISTS subqueries, complex expressions, or CTEs.
    ///
    /// # Arguments
    /// * `sql` - The raw SQL fragment to insert.
    ///
    /// # Warning
    /// This method does not provide parameter binding. Never interpolate user input
    /// directly into the SQL string. For dynamic values, use `where_raw_with()`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// let mut query = sqlx::QueryBuilder::new("SELECT * FROM items");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .where_raw("EXISTS (SELECT 1 FROM inventory WHERE inventory.item_id = items.id)")
    ///     .apply_to(&mut query);
    /// assert_eq!(
    ///     query.sql(),
    ///     "SELECT * FROM items WHERE active = $1 AND EXISTS (SELECT 1 FROM inventory WHERE inventory.item_id = items.id)"
    /// );
    /// ```
    pub fn where_raw(mut self, sql: &str) -> Self {
        let separator = self.and();
        self.conditions.push(Box::new(WhereRaw {
            sql: sql.to_string(),
            separator,
        }));
        self
    }

    /// Adds a raw SQL fragment as a WHERE condition using `OR`.
    ///
    /// Use this for static SQL fragments that don't require parameter binding.
    ///
    /// # Arguments
    /// * `sql` - The raw SQL fragment to insert.
    ///
    /// # Warning
    /// This method does not provide parameter binding. Never interpolate user input
    /// directly into the SQL string. For dynamic values, use `or_where_raw_with()`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// let mut query = sqlx::QueryBuilder::new("SELECT * FROM items");
    /// WhereBuilder::new()
    ///     .where_eq("status", "active")
    ///     .or_where_raw("(priority > 5 AND featured = true)")
    ///     .apply_to(&mut query);
    /// assert_eq!(
    ///     query.sql(),
    ///     "SELECT * FROM items WHERE status = $1 OR (priority > 5 AND featured = true)"
    /// );
    /// ```
    pub fn or_where_raw(mut self, sql: &str) -> Self {
        let separator = self.or();
        self.conditions.push(Box::new(WhereRaw {
            sql: sql.to_string(),
            separator,
        }));
        self
    }

    /// Adds a raw SQL fragment with parameter bindings using `AND`.
    ///
    /// The closure is executed at `apply_to()` time, receiving direct access to the
    /// QueryBuilder. This allows safe parameter binding using `push_bind()`.
    ///
    /// # Arguments
    /// * `callback` - A closure that receives a `QueryBuilder` to append SQL and values.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// let category_id = 42_i64;
    /// let mut query = sqlx::QueryBuilder::new("SELECT * FROM items");
    /// WhereBuilder::new()
    ///     .where_null("deleted_at")
    ///     .where_raw_with(|q| {
    ///         q.push("category_id IN (WITH RECURSIVE tree AS (SELECT id FROM categories WHERE id = ");
    ///         q.push_bind(category_id);
    ///         q.push(" UNION ALL SELECT c.id FROM categories c JOIN tree t ON c.parent_id = t.id) SELECT id FROM tree)");
    ///     })
    ///     .apply_to(&mut query);
    /// assert!(query.sql().as_str().starts_with(
    ///     "SELECT * FROM items WHERE deleted_at IS NULL AND category_id IN (WITH RECURSIVE tree AS (SELECT id FROM categories WHERE id = $1 UNION ALL"
    /// ));
    /// ```
    pub fn where_raw_with<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        let separator = self.and();
        self.conditions
            .push(Box::new(WhereRawFn::new(Box::new(callback), separator)));
        self
    }

    /// Adds a raw SQL fragment with parameter bindings using `OR`.
    ///
    /// The closure is executed at `apply_to()` time, receiving direct access to the
    /// QueryBuilder. This allows safe parameter binding using `push_bind()`.
    ///
    /// # Arguments
    /// * `callback` - A closure that receives a `QueryBuilder` to append SQL and values.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// let threshold = 100;
    /// let mut query = sqlx::QueryBuilder::new("SELECT * FROM orders");
    /// WhereBuilder::new()
    ///     .where_eq("status", "pending")
    ///     .or_where_raw_with(|q| {
    ///         q.push("(total > ");
    ///         q.push_bind(threshold);
    ///         q.push(" AND priority = 'high')");
    ///     })
    ///     .apply_to(&mut query);
    /// assert_eq!(
    ///     query.sql(),
    ///     "SELECT * FROM orders WHERE status = $1 OR (total > $2 AND priority = 'high')"
    /// );
    /// ```
    pub fn or_where_raw_with<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        let separator = self.or();
        self.conditions
            .push(Box::new(WhereRawFn::new(Box::new(callback), separator)));
        self
    }

    /// Adds a raw SQL fragment with parameter bindings using `AND`, but only if
    /// `value` is `Some`. The unwrapped value is handed to the closure along
    /// with the `QueryBuilder`. See [`where_raw_with`](Self::where_raw_with).
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// // With `None` no condition is added.
    /// let tsquery = Some("foo:*");
    /// WhereBuilder::new()
    ///     .where_raw_with_if_some(tsquery, |tsquery, q| {
    ///         q.push("search_vector @@ to_tsquery('simple', ");
    ///         q.push_bind(tsquery);
    ///         q.push(")");
    ///     })
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE search_vector @@ to_tsquery('simple', $1)");
    /// ```
    pub fn where_raw_with_if_some<T, F>(self, value: Option<T>, callback: F) -> Self
    where
        T: Send + 'a,
        F: FnOnce(T, &mut QueryBuilder<Postgres>) + Send + 'a,
    {
        match value {
            Some(value) => self.where_raw_with(move |q| callback(value, q)),
            None => self,
        }
    }

    /// Adds a raw SQL fragment with parameter bindings using `OR`, but only if
    /// `value` is `Some`. See [`where_raw_with_if_some`](Self::where_raw_with_if_some).
    pub fn or_where_raw_with_if_some<T, F>(self, value: Option<T>, callback: F) -> Self
    where
        T: Send + 'a,
        F: FnOnce(T, &mut QueryBuilder<Postgres>) + Send + 'a,
    {
        match value {
            Some(value) => self.or_where_raw_with(move |q| callback(value, q)),
            None => self,
        }
    }

    // --- EXISTS ---

    /// Adds a `WHERE EXISTS (...)` condition using `AND`, with a static SQL subquery body.
    ///
    /// Use this for subqueries that don't require parameter binding. For dynamic
    /// values, use [`where_exists_with`](Self::where_exists_with).
    ///
    /// # Arguments
    /// * `sql` - The subquery body (without the surrounding `EXISTS (...)`).
    ///
    /// # Warning
    /// This method does not provide parameter binding. Never interpolate user
    /// input directly into the SQL string.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_exists("SELECT 1 FROM inventory WHERE inventory.item_id = items.id")
    ///     .apply_to(&mut query);
    /// assert_eq!(
    ///     query.sql(),
    ///     " WHERE EXISTS (SELECT 1 FROM inventory WHERE inventory.item_id = items.id)"
    /// );
    /// ```
    pub fn where_exists(mut self, sql: &str) -> Self {
        let separator = self.and();
        self.conditions
            .push(Box::new(WhereExists::from_sql(sql.to_string(), separator)));
        self
    }

    /// Adds a `WHERE EXISTS (...)` condition using `OR`, with a static SQL subquery body.
    ///
    /// See [`where_exists`](Self::where_exists).
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("status", "active")
    ///     .or_where_exists("SELECT 1 FROM overrides WHERE overrides.item_id = items.id")
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE status = $1 OR EXISTS (SELECT 1 FROM overrides WHERE overrides.item_id = items.id)");
    /// ```
    pub fn or_where_exists(mut self, sql: &str) -> Self {
        let separator = self.or();
        self.conditions
            .push(Box::new(WhereExists::from_sql(sql.to_string(), separator)));
        self
    }

    /// Adds a `WHERE NOT EXISTS (...)` condition using `AND`, with a static SQL subquery body.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_not_exists("SELECT 1 FROM bans WHERE bans.user_id = users.id")
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE NOT EXISTS (SELECT 1 FROM bans WHERE bans.user_id = users.id)");
    /// ```
    pub fn where_not_exists(mut self, sql: &str) -> Self {
        let separator = self.and();
        self.conditions.push(Box::new(WhereNotExists::from_sql(
            sql.to_string(),
            separator,
        )));
        self
    }

    /// Adds a `WHERE NOT EXISTS (...)` condition using `OR`, with a static SQL subquery body.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("archived", true)
    ///     .or_where_not_exists("SELECT 1 FROM bans WHERE bans.user_id = users.id")
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE archived = $1 OR NOT EXISTS (SELECT 1 FROM bans WHERE bans.user_id = users.id)");
    /// ```
    pub fn or_where_not_exists(mut self, sql: &str) -> Self {
        let separator = self.or();
        self.conditions.push(Box::new(WhereNotExists::from_sql(
            sql.to_string(),
            separator,
        )));
        self
    }

    /// Adds a `WHERE EXISTS (...)` condition using `AND`, with a closure that
    /// builds the subquery body and binds parameters at apply time.
    ///
    /// The closure receives direct access to the `QueryBuilder`, so values can
    /// be bound safely with `push_bind()`.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// let owner_id = 42_i64;
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .where_exists_with(|q| {
    ///         q.push("SELECT 1 FROM memberships m WHERE m.item_id = items.id AND m.owner_id = ");
    ///         q.push_bind(owner_id);
    ///     })
    ///     .apply_to(&mut query);
    /// assert_eq!(
    ///     query.sql(),
    ///     " WHERE active = $1 AND EXISTS (SELECT 1 FROM memberships m WHERE m.item_id = items.id AND m.owner_id = $2)"
    /// );
    /// ```
    pub fn where_exists_with<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereExists::from_builder(
            Box::new(callback),
            separator,
        )));
        self
    }

    /// Adds a `WHERE EXISTS (...)` condition using `OR`, with a parameter-binding closure.
    ///
    /// See [`where_exists_with`](Self::where_exists_with).
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// let user_id = 42;
    /// // " WHERE active = $1 OR EXISTS (SELECT 1 FROM admins WHERE admins.user_id = $2)"
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .or_where_exists_with(|q| {
    ///         q.push("SELECT 1 FROM admins WHERE admins.user_id = ");
    ///         q.push_bind(user_id);
    ///     })
    ///     .apply_to(&mut query);
    /// ```
    pub fn or_where_exists_with<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereExists::from_builder(
            Box::new(callback),
            separator,
        )));
        self
    }

    /// Adds a `WHERE NOT EXISTS (...)` condition using `AND`, with a parameter-binding closure.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("active", true)
    ///     .where_not_exists_with(|q| {
    ///         q.push("SELECT 1 FROM bans WHERE bans.user_id = users.id AND bans.reason = ");
    ///         q.push_bind("spam");
    ///     })
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE active = $1 AND NOT EXISTS (SELECT 1 FROM bans WHERE bans.user_id = users.id AND bans.reason = $2)");
    /// ```
    pub fn where_not_exists_with<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        let separator = self.and();
        self.conditions.push(Box::new(WhereNotExists::from_builder(
            Box::new(callback),
            separator,
        )));
        self
    }

    /// Adds a `WHERE NOT EXISTS (...)` condition using `OR`, with a parameter-binding closure.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// WhereBuilder::new()
    ///     .where_eq("archived", true)
    ///     .or_where_not_exists_with(|q| {
    ///         q.push("SELECT 1 FROM bans WHERE bans.user_id = users.id AND bans.reason = ");
    ///         q.push_bind("spam");
    ///     })
    ///     .apply_to(&mut query);
    /// assert_eq!(query.sql(), " WHERE archived = $1 OR NOT EXISTS (SELECT 1 FROM bans WHERE bans.user_id = users.id AND bans.reason = $2)");
    /// ```
    pub fn or_where_not_exists_with<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        let separator = self.or();
        self.conditions.push(Box::new(WhereNotExists::from_builder(
            Box::new(callback),
            separator,
        )));
        self
    }

    // --- Applying the Clause ---

    /// Applies the built WHERE clause to a `sqlx::QueryBuilder`.
    /// If no conditions were added, this method does nothing and returns `false`.
    /// Otherwise, it prepends "WHERE " and applies all conditions, returning `true`.
    ///
    /// Returns `true` if the WHERE clause was applied, `false` otherwise.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// let mut query = sqlx::QueryBuilder::new("SELECT * FROM users");
    /// let applied = WhereBuilder::new()
    ///     .where_eq("status", "active")
    ///     .apply_to(&mut query);
    /// assert!(applied);
    /// assert_eq!(query.sql(), "SELECT * FROM users WHERE status = $1");
    ///
    /// let mut empty_q = sqlx::QueryBuilder::new("SELECT 1");
    /// let applied = WhereBuilder::new().apply_to(&mut empty_q);
    /// assert!(!applied);
    /// assert_eq!(empty_q.sql(), "SELECT 1");
    /// ```
    pub fn apply_to(self, query: &mut QueryBuilder<Postgres>) -> bool {
        if self.conditions.is_empty() {
            return false;
        }

        query.push(" WHERE ");
        self.apply_predicate(query);

        true
    }

    /// Apply just the predicate body, no leading ` WHERE `. Used when the
    /// same predicate machinery is embedded elsewhere: `JOIN … ON <p>`,
    /// `HAVING <p>`, the body of a `CHECK` constraint, etc.
    ///
    /// Returns `true` if any conditions were written, `false` if the builder
    /// was empty.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// let mut query = sqlx::QueryBuilder::new(
    ///     "SELECT * FROM orders o JOIN line_items l ON ",
    /// );
    /// WhereBuilder::new()
    ///     .where_col("l.order_id", Equal, "o.id")
    ///     .where_eq("l.refunded", false)
    ///     .apply_predicate(&mut query);
    /// assert_eq!(
    ///     query.sql(),
    ///     "SELECT * FROM orders o JOIN line_items l ON l.order_id = $1 AND l.refunded = $2"
    /// );
    /// ```
    pub fn apply_predicate(self, query: &mut QueryBuilder<Postgres>) -> bool {
        if self.conditions.is_empty() {
            return false;
        }

        let mixed = self.mixed();
        apply_conditions(self.conditions, mixed, query);

        true
    }

    /// Checks if any conditions have been added to the builder.
    ///
    /// # Example
    /// ```
    /// # use pgkit::query_builder::WhereBuilder;
    /// # use pgkit::query_builder::operators::*;
    /// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    /// assert!(WhereBuilder::new().is_empty());
    /// assert!(!WhereBuilder::new().where_eq("col", 1).is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }

    /// True when the clause filters nothing: either it has no conditions, or
    /// every condition renders as a constant `TRUE` (an empty `NOT IN` / `<> ALL`
    /// list). The whole-table write guards check this rather than `is_empty`,
    /// which such a clause would otherwise pass.
    pub fn is_unfiltered(&self) -> bool {
        self.conditions
            .iter()
            .all(|condition| condition.is_unconditional())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_col_applies_single_where_condition() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_col("name", Equal, "rustacean")
            .apply_to(&mut query);

        assert_eq!(query.sql(), (" WHERE name = $1"));
    }

    #[test]
    fn where_col_applies_many_where_conditions() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_col("name", Equal, "rustacean")
            .where_col("age", GreaterThan, 16)
            .where_col("active", NotEqual, false)
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE name = $1 AND age > $2 AND active <> $3"
        );
    }

    #[test]
    fn or_where_col_applies_many_where_condition() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .or_where_col("name", Equal, "rustacean")
            .or_where_col("age", GreaterThan, 16)
            .or_where_col("active", NotEqual, false)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE name = $1 OR age > $2 OR active <> $3");
    }

    #[test]
    fn where_eq_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("status", "active")
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE status = $1");
    }

    #[test]
    fn where_ne_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_ne("status", "inactive")
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE status <> $1");
    }

    #[test]
    fn where_gt_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_gt("score", 90)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE score > $1");
    }

    #[test]
    fn where_lt_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_lt("attempts", 3)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE attempts < $1");
    }

    #[test]
    fn where_gte_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_gte("age", 18)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE age >= $1");
    }

    #[test]
    fn where_lte_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_lte("price", 100.0)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE price <= $1");
    }

    #[test]
    fn where_null_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_null("deleted_at")
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE deleted_at IS NULL");
    }

    #[test]
    fn where_not_null_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_not_null("deleted_at")
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE deleted_at IS NOT NULL");
    }

    #[test]
    fn where_in_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_in("status", vec!["active", "pending"])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE status IN ($1, $2)");
    }

    #[test]
    fn where_not_in_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_not_in("status", vec!["inactive", "banned"])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE status NOT IN ($1, $2)");
    }

    #[test]
    fn where_between_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_between("age", 18, 30)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE age BETWEEN $1 AND $2");
    }

    #[test]
    fn or_where_between_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .or_where_between("age", 18, 30)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE age BETWEEN $1 AND $2");
    }

    #[test]
    fn where_not_between_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_not_between("age", 18, 30)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE age NOT BETWEEN $1 AND $2");
    }

    #[test]
    fn or_where_not_between_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .or_where_not_between("age", 18, 30)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE age NOT BETWEEN $1 AND $2");
    }

    #[test]
    fn where_col_if_some_generates_correct_sql_when_some() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_col_if_some("status", Equal, Some("active"))
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE status = $1");
    }

    #[test]
    fn where_col_if_some_skips_condition_when_none() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_col_if_some("status", Equal, None::<&str>)
            .apply_to(&mut query);

        assert_eq!(query.sql(), "");
    }

    #[test]
    fn when_generates_correct_sql_when_condition_is_true() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .when(true, |b| b.where_eq("status", "active"))
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE status = $1");
    }

    #[test]
    fn when_skips_condition_when_condition_is_false() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .when(false, |b| b.where_eq("status", "active"))
            .apply_to(&mut query);

        assert_eq!(query.sql(), "");
    }

    #[test]
    fn multiple_conditions_with_and_and_or_generate_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("status", "active")
            .or_where_eq("role", "admin")
            .where_gt("age", 18)
            .apply_to(&mut query);

        // Mixed AND/OR renders left-associatively parenthesized so the SQL
        // evaluates in call order; without the parens, `AND` would bind
        // tighter and regroup this as `status OR (role AND age)`.
        assert_eq!(
            query.sql(),
            " WHERE ((status = $1 OR role = $2) AND age > $3)"
        );
    }

    #[test]
    fn uniform_or_chain_stays_flat() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("a", 1)
            .or_where_eq("b", 2)
            .or_where_eq("c", 3)
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE a = $1 OR b = $2 OR c = $3");
    }

    #[test]
    fn mixed_and_or_inside_group_parenthesizes() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("x", 0)
            .group(|b| b.where_eq("a", 1).or_where_eq("b", 2).where_gt("c", 3))
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE x = $1 AND (((a = $2 OR b = $3) AND c > $4))"
        );
    }

    #[test]
    fn empty_where_clause_does_not_generate_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        let applied = WhereBuilder::new().apply_to(&mut query);

        assert!(!applied);
        assert_eq!(query.sql(), "");
    }

    #[test]
    fn where_in_with_empty_vec_matches_nothing() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_in::<_, String>("status", vec![])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE FALSE");
    }

    #[test]
    fn or_where_in_with_empty_vec_matches_nothing() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("active", true)
            .or_where_in::<_, String>("status", vec![])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE active = $1 OR FALSE");
    }

    #[test]
    fn where_not_in_with_empty_vec_matches_everything() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_not_in::<_, String>("status", vec![])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE TRUE");
    }

    #[test]
    fn or_where_not_in_with_empty_vec_matches_everything() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("active", true)
            .or_where_not_in::<_, String>("status", vec![])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE active = $1 OR TRUE");
    }

    #[test]
    fn grouping_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .group(|b| b.where_eq("name", "rustacean").where_gt("age", 16))
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE (name = $1 AND age > $2)");

        query.reset();

        WhereBuilder::new()
            .where_eq("department", "software")
            .group(|b| b.where_eq("name", "rustacean").where_gt("age", 16))
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE department = $1 AND (name = $2 AND age > $3)"
        );

        query.reset();

        WhereBuilder::new()
            .group(|b| b.where_eq("name", "rustacean").where_gt("age", 16))
            .or_group(|b| b.where_ne("name", "rustacean").or_where_lte("age", 16))
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE (name = $1 AND age > $2) OR (name <> $3 OR age <= $4)"
        );
    }

    #[test]
    fn nested_grouping_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("a", 1)
            .group(|b| {
                b.where_eq("b", 2)
                    .or_group(|nb| nb.where_eq("c", 3).where_null("d"))
            })
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE a = $1 AND (b = $2 OR (c = $3 AND d IS NULL))"
        );
    }

    #[test]
    fn or_group_after_where_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("a", 1)
            .or_group(|b| b.where_eq("b", 2).where_eq("c", 3))
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE a = $1 OR (b = $2 AND c = $3)");
    }

    #[test]
    fn is_empty_returns_correct_state() {
        let builder_empty = WhereBuilder::new();
        assert!(builder_empty.is_empty());

        let builder_not_empty = WhereBuilder::new().where_eq("a", 1);
        assert!(!builder_not_empty.is_empty());

        // An empty IN-list is a real condition (FALSE), not a no-op.
        let builder_empty_vec = WhereBuilder::new().where_in::<_, i32>("b", vec![]);
        assert!(!builder_empty_vec.is_empty());

        let builder_some_vec = WhereBuilder::new().where_in("b", vec![1]);
        assert!(!builder_some_vec.is_empty());
    }

    // --- Raw SQL Tests ---

    #[test]
    fn where_raw_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_raw("EXISTS (SELECT 1 FROM inventory WHERE item_id = items.id)")
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE EXISTS (SELECT 1 FROM inventory WHERE item_id = items.id)"
        );
    }

    #[test]
    fn or_where_raw_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("status", "active")
            .or_where_raw("(priority > 5 AND featured = true)")
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE status = $1 OR (priority > 5 AND featured = true)"
        );
    }

    #[test]
    fn where_raw_combines_with_other_conditions() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_null("deleted_at")
            .where_raw("category_id IN (SELECT id FROM active_categories)")
            .where_eq("is_featured", true)
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE deleted_at IS NULL AND category_id IN (SELECT id FROM active_categories) AND is_featured = $1"
        );
    }

    #[test]
    fn where_raw_with_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_raw_with(|q| {
                q.push("price > ");
                q.push_bind(100);
            })
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE price > $1");
    }

    #[test]
    fn or_where_raw_with_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("status", "pending")
            .or_where_raw_with(|q| {
                q.push("(total > ");
                q.push_bind(500);
                q.push(" AND priority = 'high')");
            })
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE status = $1 OR (total > $2 AND priority = 'high')"
        );
    }

    #[test]
    fn where_raw_with_if_some_applies_when_some() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("active", true)
            .where_raw_with_if_some(Some("foo:*"), |search, q| {
                q.push("search_vector @@ to_tsquery('simple', ");
                q.push_bind(search);
                q.push(")");
            })
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE active = $1 AND search_vector @@ to_tsquery('simple', $2)"
        );
    }

    #[test]
    fn where_raw_with_if_some_skips_when_none() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("active", true)
            .where_raw_with_if_some(None::<&str>, |search, q| {
                q.push("search_vector @@ to_tsquery('simple', ");
                q.push_bind(search);
                q.push(")");
            })
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE active = $1");
    }

    #[test]
    fn where_raw_with_multiple_values() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_raw_with(|q| {
                q.push("created_at BETWEEN ");
                q.push_bind("2024-01-01");
                q.push(" AND ");
                q.push_bind("2024-12-31");
            })
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE created_at BETWEEN $1 AND $2");
    }

    #[test]
    fn where_raw_with_combines_with_standard_conditions() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_null("deleted_at")
            .where_eq("is_active", true)
            .where_raw_with(|q| {
                q.push("category_id IN (WITH RECURSIVE tree AS (SELECT id FROM categories WHERE parent_id = ");
                q.push_bind(42);
                q.push(") SELECT id FROM tree)");
            })
            .where_gt("price", 0)
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE deleted_at IS NULL AND is_active = $1 AND category_id IN (WITH RECURSIVE tree AS (SELECT id FROM categories WHERE parent_id = $2) SELECT id FROM tree) AND price > $3"
        );
    }

    #[test]
    fn where_raw_in_group() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("status", "active")
            .group(|b| {
                b.where_raw("inventory_count > 0")
                    .or_where_raw("is_preorder = true")
            })
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE status = $1 AND (inventory_count > 0 OR is_preorder = true)"
        );
    }

    // --- ANY / ALL Tests ---

    #[test]
    fn where_any_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_any("status", vec!["active", "pending"])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE status = ANY($1)");
    }

    #[test]
    fn or_where_any_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("active", true)
            .or_where_any("status", vec!["pending"])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE active = $1 OR status = ANY($2)");
    }

    #[test]
    fn where_any_op_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_any_op("score", GreaterThan, vec![10, 20, 30])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE score > ANY($1)");
    }

    #[test]
    fn where_any_with_empty_vec_binds_empty_array() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_any::<_, String>("status", vec![])
            .apply_to(&mut query);

        // `= ANY('{}')` is FALSE in Postgres, matches nothing.
        assert_eq!(query.sql(), " WHERE status = ANY($1)");
    }

    #[test]
    fn where_all_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_all("score", GreaterThan, vec![10, 20, 30])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE score > ALL($1)");
    }

    #[test]
    fn or_where_all_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("active", true)
            .or_where_all("score", NotEqual, vec![0])
            .apply_to(&mut query);

        assert_eq!(query.sql(), " WHERE active = $1 OR score <> ALL($2)");
    }

    #[test]
    fn where_all_with_empty_vec_binds_empty_array() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_all::<_, i32>("score", GreaterThan, vec![])
            .apply_to(&mut query);

        // `op ALL('{}')` is TRUE in Postgres, matches everything.
        assert_eq!(query.sql(), " WHERE score > ALL($1)");
    }

    // --- EXISTS Tests ---

    #[test]
    fn where_exists_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_exists("SELECT 1 FROM inventory WHERE inventory.item_id = items.id")
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE EXISTS (SELECT 1 FROM inventory WHERE inventory.item_id = items.id)"
        );
    }

    #[test]
    fn where_not_exists_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_not_exists("SELECT 1 FROM bans WHERE bans.user_id = users.id")
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE NOT EXISTS (SELECT 1 FROM bans WHERE bans.user_id = users.id)"
        );
    }

    #[test]
    fn or_where_exists_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("status", "active")
            .or_where_exists("SELECT 1 FROM overrides WHERE overrides.item_id = items.id")
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE status = $1 OR EXISTS (SELECT 1 FROM overrides WHERE overrides.item_id = items.id)"
        );
    }

    #[test]
    fn where_exists_with_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_exists_with(|q| {
                q.push("SELECT 1 FROM memberships m WHERE m.item_id = items.id AND m.owner_id = ");
                q.push_bind(42);
            })
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE EXISTS (SELECT 1 FROM memberships m WHERE m.item_id = items.id AND m.owner_id = $1)"
        );
    }

    #[test]
    fn where_not_exists_with_generates_correct_sql() {
        let mut query = sqlx::QueryBuilder::new("");

        WhereBuilder::new()
            .where_eq("active", true)
            .where_not_exists_with(|q| {
                q.push("SELECT 1 FROM bans WHERE bans.user_id = users.id AND bans.lifted_at IS NULL AND bans.reason = ");
                q.push_bind("spam");
            })
            .apply_to(&mut query);

        assert_eq!(
            query.sql(),
            " WHERE active = $1 AND NOT EXISTS (SELECT 1 FROM bans WHERE bans.user_id = users.id AND bans.lifted_at IS NULL AND bans.reason = $2)"
        );
    }
}
