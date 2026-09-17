use sqlx::{Postgres, QueryBuilder};

use super::some_where_condition::{SomeWhereCondition, apply_conditions};
use crate::query_builder::operators::LogicalOperator;

/// Grouped conditions enclosed in parentheses.
/// e.g., `(condition1 AND condition2)` or `(condition1 OR condition2)`.
///
/// The group's own `separator` controls how it joins to the surrounding clause;
/// the inner conditions carry their own separators (`AND`/`OR`) between
/// themselves. When the inner conditions mix `AND` and `OR` (`mixed`), they
/// render left-associatively parenthesized, like the top-level clause.
///
/// # Example
/// ```ignore
/// // Produces: ` OR (name = $1 AND age > $2)`
/// WhereGroup {
///     conditions: vec![
///         Box::new(WhereCondition {
///             column: "name".into(),
///             operator: ComparisonOperator::Equal,
///             value: "rustacean",
///             separator: None,
///         }),
///         Box::new(WhereCondition {
///             column: "age".into(),
///             operator: ComparisonOperator::GreaterThan,
///             value: 16,
///             separator: Some(LogicalOperator::And),
///         }),
///     ],
///     separator: Some(LogicalOperator::Or),
///     mixed: false,
/// };
/// ```
pub struct WhereGroup<'a> {
    /// The conditions contained within this group.
    pub conditions: Vec<Box<dyn SomeWhereCondition<'a> + 'a + Send>>,
    /// The logical operator (`AND` or `OR`).
    pub separator: Option<LogicalOperator>,
    /// Whether the inner conditions mix `AND` and `OR`.
    pub mixed: bool,
}

impl<'a> SomeWhereCondition<'a> for WhereGroup<'a> {
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        query.push("(");
        apply_conditions(self.conditions, self.mixed, query);
        query.push(")");
    }
}
