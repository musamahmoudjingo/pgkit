use sqlx::{Postgres, QueryBuilder};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::LogicalOperator;

pub type ExistsBuilder<'a> = Box<dyn FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a>;

/// The body of an `EXISTS (...)` subquery: either a static SQL fragment or a
/// deferred closure that binds parameters at apply time.
pub enum ExistsBody<'a> {
    /// Static SQL for the subquery body (without the surrounding `EXISTS (...)`).
    Sql(String),
    /// Deferred subquery builder, called by value at `apply_to` time.
    Builder(ExistsBuilder<'a>),
}

/// `WHERE EXISTS (...)`.
///
/// The body is either a static SQL string or a deferred closure with access
/// to the outer `QueryBuilder` for safe parameter binding via `push_bind()`.
/// Use the closure form when the subquery has parameters.
///
/// # Examples
///
/// Static subquery body (no parameters):
/// ```ignore
/// // Produces: ` AND EXISTS (SELECT 1 FROM inventory WHERE inventory.item_id = items.id)`
/// WhereExists::from_sql(
///     "SELECT 1 FROM inventory WHERE inventory.item_id = items.id".to_string(),
///     Some(LogicalOperator::And),
/// );
/// ```
///
/// Deferred body with parameter binding:
/// ```ignore
/// // Produces: ` AND EXISTS (SELECT 1 FROM memberships m WHERE m.item_id = items.id AND m.owner_id = $1)`
/// let owner_id = 42;
/// WhereExists::from_builder(
///     Box::new(move |q| {
///         q.push("SELECT 1 FROM memberships m WHERE m.item_id = items.id AND m.owner_id = ");
///         q.push_bind(owner_id);
///     }),
///     Some(LogicalOperator::And),
/// );
/// ```
pub struct WhereExists<'a> {
    pub(crate) body: ExistsBody<'a>,
    pub(crate) separator: Option<LogicalOperator>,
}

impl<'a> WhereExists<'a> {
    pub(crate) fn from_sql(sql: String, separator: Option<LogicalOperator>) -> Self {
        Self {
            body: ExistsBody::Sql(sql),
            separator,
        }
    }

    pub(crate) fn from_builder(
        builder: ExistsBuilder<'a>,
        separator: Option<LogicalOperator>,
    ) -> Self {
        Self {
            body: ExistsBody::Builder(builder),
            separator,
        }
    }
}

impl<'a> SomeWhereCondition<'a> for WhereExists<'a> {
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push("EXISTS (");

        match self.body {
            ExistsBody::Sql(sql) => {
                query.push(sql);
            }
            ExistsBody::Builder(builder) => builder(query),
        }

        query.push(")");
    }
}
