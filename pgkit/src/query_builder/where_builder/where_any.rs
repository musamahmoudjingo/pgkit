use sqlx::postgres::PgHasArrayType;
use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{ComparisonOperator, LogicalOperator};
use crate::types::ColumnRef;

/// `WHERE column op ANY($1)`. `$1` is bound as a
/// PostgreSQL array containing all of the supplied values.
///
/// Holds when `operator` is satisfied against *at least one* element of the
/// array, e.g. `column = ANY(arr)` (equivalent to `IN`), `column > ANY(arr)`
/// (greater than at least one element).
///
/// An empty `value` binds an empty array; `op ANY('{}')` is `FALSE` in
/// Postgres, so an empty list matches nothing, exact SQL semantics.
///
/// # Example
/// ```ignore
/// // Produces: ` AND status = ANY($1)`
/// WhereAny {
///     column: "status".into(),
///     operator: ComparisonOperator::Equal,
///     value: vec!["active", "pending"],
///     separator: Some(LogicalOperator::And),
/// };
/// ```
pub struct WhereAny<T> {
    pub column: ColumnRef,
    pub operator: ComparisonOperator,
    pub value: Vec<T>,
    pub separator: Option<LogicalOperator>,
}

impl<'a, T> SomeWhereCondition<'a> for WhereAny<T>
where
    T: Encode<'a, Postgres> + Type<Postgres> + PgHasArrayType + Send + 'a,
{
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push(&self.column);
        query.push(self.operator);
        query.push("ANY(");
        query.push_bind(self.value);
        query.push(")");
    }
}
