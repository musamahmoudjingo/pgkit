use sqlx::postgres::PgHasArrayType;
use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{ComparisonOperator, LogicalOperator};
use crate::types::ColumnRef;

/// `WHERE column op ALL($1)`. `$1` is bound as a
/// PostgreSQL array containing all of the supplied values.
///
/// Holds when `operator` is satisfied against *every* element of the array,
/// e.g. `column > ALL(arr)` (strictly greater than every element),
/// `column <> ALL(arr)` (equivalent to `NOT IN`).
///
/// An empty `value` binds an empty array; `op ALL('{}')` is `TRUE` in
/// Postgres, so an empty list matches everything, exact SQL semantics.
///
/// # Example
/// ```ignore
/// // Produces: ` AND score > ALL($1)`
/// WhereAll {
///     column: "score".into(),
///     operator: ComparisonOperator::GreaterThan,
///     value: vec![10, 20, 30],
///     separator: Some(LogicalOperator::And),
/// };
/// ```
pub struct WhereAll<T> {
    pub column: ColumnRef,
    pub operator: ComparisonOperator,
    pub value: Vec<T>,
    pub separator: Option<LogicalOperator>,
}

impl<'a, T> SomeWhereCondition<'a> for WhereAll<T>
where
    T: Encode<'a, Postgres> + Type<Postgres> + PgHasArrayType + Send + 'a,
{
    fn is_unconditional(&self) -> bool {
        self.value.is_empty()
    }

    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push(&self.column);
        query.push(self.operator);
        query.push("ALL(");
        query.push_bind(self.value);
        query.push(")");
    }
}
