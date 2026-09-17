use sqlx::{Postgres, QueryBuilder};

/// Any condition that can be applied to a WHERE clause.
///
/// # Example
/// ```ignore
/// // A custom condition that emits `column ILIKE $1`.
/// struct WhereILike {
///     column: ColumnRef,
///     value: String,
///     separator: Option<LogicalOperator>,
/// }
///
/// impl<'a> SomeWhereCondition<'a> for WhereILike {
///     fn apply_to(self: Box<Self>, query: &mut sqlx::QueryBuilder<sqlx::Postgres>) {
///         if let Some(sep) = self.separator {
///             query.push(sep);
///         }
///         query.push(&self.column);
///         query.push(" ILIKE ");
///         query.push_bind(self.value);
///     }
/// }
/// ```
pub trait SomeWhereCondition<'a> {
    /// Applies the specific condition logic to the given `sqlx::QueryBuilder`.
    ///
    /// Adds the necessary SQL fragment (e.g., "column = $1", "column IS NULL")
    /// and binds any required parameters to the query.
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>);

    /// Whether this condition matches every row, so a WHERE clause made only of
    /// such conditions is no filter at all. An empty `NOT IN` / `<> ALL` list
    /// renders as a constant `TRUE`, which would otherwise satisfy the
    /// whole-table write guard on `UpdateBuilder` / `DeleteBuilder`.
    fn is_unconditional(&self) -> bool {
        false
    }
}

/// Render a condition list. When `mixed` (both `AND` and `OR` separators were
/// used at this level), conditions are parenthesized left-associatively,
/// `((c1 OR c2) AND c3)`, so the SQL evaluates in call order instead of
/// letting `AND`'s higher precedence regroup it.
pub(super) fn apply_conditions<'a>(
    conditions: Vec<Box<dyn SomeWhereCondition<'a> + 'a + Send>>,
    mixed: bool,
    query: &mut QueryBuilder<Postgres>,
) {
    if mixed && conditions.len() > 1 {
        for _ in 1..conditions.len() {
            query.push("(");
        }
        for (i, condition) in conditions.into_iter().enumerate() {
            condition.apply_to(query);
            if i > 0 {
                query.push(")");
            }
        }
    } else {
        for condition in conditions {
            condition.apply_to(query);
        }
    }
}
