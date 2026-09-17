use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{ComparisonOperator, LogicalOperator};
use crate::types::ColumnRef;

/// Basic WHERE condition comparing a column to a value using an operator.
/// e.g., `column = value`, `column > value`.
///
/// # Example
/// ```ignore
/// // Produces: ` AND age > $1`
/// WhereCondition {
///     column: "age".into(),
///     operator: ComparisonOperator::GreaterThan,
///     value: 18,
///     separator: Some(LogicalOperator::And),
/// };
/// ```
pub struct WhereCondition<T> {
    pub column: ColumnRef,
    pub operator: ComparisonOperator,
    pub value: T,
    pub separator: Option<LogicalOperator>,
}

impl<'a, T> SomeWhereCondition<'a> for WhereCondition<T>
where
    T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
{
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push(&self.column);
        query.push(self.operator);
        query.push_bind(self.value);
    }
}
