use sqlx::{Postgres, QueryBuilder};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{ComparisonOperator, LogicalOperator};
use crate::types::ColumnRef;

/// WHERE condition comparing a column to another column, e.g.
/// `l.order_id = o.id`. Both sides render as column references; nothing is
/// bound as a parameter (that is [`super::WhereCondition`]'s job).
pub struct WhereColumnPair {
    pub left: ColumnRef,
    pub operator: ComparisonOperator,
    pub right: ColumnRef,
    pub separator: Option<LogicalOperator>,
}

impl<'a> SomeWhereCondition<'a> for WhereColumnPair {
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push(&self.left);
        query.push(self.operator);
        query.push(&self.right);
    }
}
