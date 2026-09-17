use sqlx::{Postgres, QueryBuilder};

use super::some_where_condition::SomeWhereCondition;
use super::where_exists::{ExistsBody, ExistsBuilder};
use crate::query_builder::operators::LogicalOperator;

/// `WHERE NOT EXISTS (...)`.
///
/// The body is either a static SQL string or a deferred closure with access
/// to the outer `QueryBuilder` for safe parameter binding via `push_bind()`.
/// Use the closure form when the subquery has parameters.
///
/// # Examples
///
/// Static subquery body (no parameters):
/// ```ignore
/// // Produces: ` AND NOT EXISTS (SELECT 1 FROM bans WHERE bans.user_id = users.id)`
/// WhereNotExists::from_sql(
///     "SELECT 1 FROM bans WHERE bans.user_id = users.id".to_string(),
///     Some(LogicalOperator::And),
/// );
/// ```
///
/// Deferred body with parameter binding:
/// ```ignore
/// // Produces: ` AND NOT EXISTS (SELECT 1 FROM bans WHERE bans.user_id = users.id AND bans.reason = $1)`
/// WhereNotExists::from_builder(
///     Box::new(|q| {
///         q.push("SELECT 1 FROM bans WHERE bans.user_id = users.id AND bans.reason = ");
///         q.push_bind("spam");
///     }),
///     Some(LogicalOperator::And),
/// );
/// ```
pub struct WhereNotExists<'a> {
    pub(crate) body: ExistsBody<'a>,
    pub(crate) separator: Option<LogicalOperator>,
}

impl<'a> WhereNotExists<'a> {
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

impl<'a> SomeWhereCondition<'a> for WhereNotExists<'a> {
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push("NOT EXISTS (");

        match self.body {
            ExistsBody::Sql(sql) => {
                query.push(sql);
            }
            ExistsBody::Builder(builder) => builder(query),
        }

        query.push(")");
    }
}
