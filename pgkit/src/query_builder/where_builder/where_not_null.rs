use sqlx::{Postgres, QueryBuilder};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{LogicalOperator, NullOperator};
use crate::types::ColumnRef;

/// `WHERE column IS NOT NULL` condition.
///
/// # Example
/// ```ignore
/// // Produces: ` AND deleted_at IS NOT NULL`
/// WhereNotNull {
///     column: "deleted_at".into(),
///     separator: Some(LogicalOperator::And),
/// };
/// ```
pub struct WhereNotNull {
    pub column: ColumnRef,
    pub separator: Option<LogicalOperator>,
}

impl<'a> SomeWhereCondition<'a> for WhereNotNull {
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push(&self.column);
        query.push(NullOperator::IsNotNull);
    }
}
