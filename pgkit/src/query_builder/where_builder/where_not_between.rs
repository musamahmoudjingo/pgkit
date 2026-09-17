use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{LogicalOperator, RangeOperator};
use crate::types::ColumnRef;

/// `WHERE column NOT BETWEEN lower AND upper`.
///
/// Equivalent to `column < lower OR column > upper`.
///
/// # Example
/// ```ignore
/// // Produces: ` AND age NOT BETWEEN $1 AND $2`
/// WhereNotBetween {
///     column: "age".into(),
///     lower: 18,
///     upper: 30,
///     separator: Some(LogicalOperator::And),
/// };
/// ```
pub struct WhereNotBetween<T> {
    pub column: ColumnRef,
    pub lower: T,
    pub upper: T,
    pub separator: Option<LogicalOperator>,
}

impl<'a, T> SomeWhereCondition<'a> for WhereNotBetween<T>
where
    T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
{
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push(&self.column);
        query.push(RangeOperator::NotBetween);
        query.push_bind(self.lower);
        query.push(LogicalOperator::And);
        query.push_bind(self.upper);
    }
}
