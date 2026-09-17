use sqlx::{Postgres, QueryBuilder};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::LogicalOperator;

/// Type alias for the closure used in `WhereRawFn` to build SQL fragments with parameter binding.
pub type RawFnBuilder<'a> = Box<dyn FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a>;

/// A raw SQL fragment built via a deferred closure.
///
/// The closure is executed at apply_to time, allowing direct access to the
/// QueryBuilder for safe parameter binding with `push_bind()`.
///
/// # Example
/// ```ignore
/// // Produces: ` AND price > $1`
/// WhereRawFn::new(
///     Box::new(|q| {
///         q.push("price > ");
///         q.push_bind(100);
///     }),
///     Some(LogicalOperator::And),
/// );
/// ```
pub struct WhereRawFn<'a> {
    pub(crate) builder: RawFnBuilder<'a>,
    pub(crate) separator: Option<LogicalOperator>,
}

impl<'a> WhereRawFn<'a> {
    pub(crate) fn new(builder: RawFnBuilder<'a>, separator: Option<LogicalOperator>) -> Self {
        Self { builder, separator }
    }
}

impl<'a> SomeWhereCondition<'a> for WhereRawFn<'a> {
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }
        (self.builder)(query);
    }
}
