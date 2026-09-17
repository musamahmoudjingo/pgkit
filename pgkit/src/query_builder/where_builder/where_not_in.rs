use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{LogicalOperator, SetOperator};
use crate::types::ColumnRef;

/// `WHERE column NOT IN (value1, value2, ...)`.
///
/// Each element is bound as a separate parameter. For large lists, prefer
/// [`WhereAll`](super::WhereAll) with `<>`, which binds the entire list as a
/// single PostgreSQL array.
///
/// If `value` is empty, emits `TRUE`: exclusion from an empty set matches
/// everything, mirroring SQL semantics. (`NOT IN ()` itself is a Postgres
/// syntax error, so the equivalent constant is emitted instead.)
///
/// # Example
/// ```ignore
/// // Produces: ` AND status NOT IN ($1, $2)`
/// WhereNotIn {
///     column: "status".into(),
///     value: vec!["inactive", "banned"],
///     separator: Some(LogicalOperator::And),
/// };
/// ```
pub struct WhereNotIn<T> {
    pub column: ColumnRef,
    pub value: Vec<T>,
    pub separator: Option<LogicalOperator>,
}

impl<'a, T> SomeWhereCondition<'a> for WhereNotIn<T>
where
    T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
{
    fn is_unconditional(&self) -> bool {
        self.value.is_empty()
    }

    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        if self.value.is_empty() {
            query.push("TRUE");
            return;
        }

        query.push(&self.column);
        query.push(SetOperator::NotIn);
        query.push("(");
        let mut separated = query.separated(", ");
        for value in self.value {
            separated.push_bind(value);
        }
        query.push(")");
    }
}
