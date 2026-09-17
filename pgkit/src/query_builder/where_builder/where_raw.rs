use sqlx::{Postgres, QueryBuilder};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::LogicalOperator;

/// Raw SQL fragment condition.
///
/// Use this for static SQL fragments that don't require parameter binding.
/// For dynamic values with parameter binding, use `WhereRawFn` instead.
///
/// # Example
/// ```ignore
/// WhereRaw {
///     sql: "EXISTS (SELECT 1 FROM users WHERE active = true)".to_string(),
///     separator: Some(LogicalOperator::And),
/// }
/// ```
pub struct WhereRaw {
    /// The raw SQL fragment to insert.
    pub sql: String,
    /// The logical operator (`AND` or `OR`).
    pub separator: Option<LogicalOperator>,
}

impl<'a> SomeWhereCondition<'a> for WhereRaw {
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }
        query.push(self.sql);
    }
}
