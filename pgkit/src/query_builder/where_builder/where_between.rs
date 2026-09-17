use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{LogicalOperator, RangeOperator};
use crate::types::ColumnRef;

/// `WHERE column BETWEEN lower AND upper`.
///
/// Inclusive on both ends, equivalent to `column >= lower AND column <= upper`.
///
/// # Example
/// ```ignore
/// // Produces: ` AND age BETWEEN $1 AND $2`
/// WhereBetween {
///     column: "age".into(),
///     lower: 18,
///     upper: 30,
///     separator: Some(LogicalOperator::And),
/// };
/// ```
pub struct WhereBetween<T> {
    pub column: ColumnRef,
    pub lower: T,
    pub upper: T,
    pub separator: Option<LogicalOperator>,
}

impl<'a, T> SomeWhereCondition<'a> for WhereBetween<T>
where
    T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
{
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push(&self.column);
        query.push(RangeOperator::Between);
        query.push_bind(self.lower);
        query.push(LogicalOperator::And);
        query.push_bind(self.upper);
    }
}
