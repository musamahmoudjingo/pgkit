use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::some_where_condition::SomeWhereCondition;
use crate::query_builder::operators::{LogicalOperator, SetOperator};
use crate::types::ColumnRef;

/// `WHERE column IN (value1, value2, ...)`.
///
/// Each element is bound as a separate parameter. For large lists, prefer
/// [`WhereAny`](super::WhereAny) with `=`, which binds the entire list as a
/// single PostgreSQL array.
///
/// If `value` is empty, emits `FALSE`: membership in an empty set matches
/// nothing, mirroring SQL semantics. (`IN ()` itself is a Postgres syntax
/// error, so the equivalent constant is emitted instead.)
///
/// # Example
/// ```ignore
/// // Produces: ` AND status IN ($1, $2)`
/// WhereIn {
///     column: "status".into(),
///     value: vec!["active", "pending"],
///     separator: Some(LogicalOperator::And),
/// };
/// ```
pub struct WhereIn<T> {
    pub column: ColumnRef,
    pub value: Vec<T>,
    pub separator: Option<LogicalOperator>,
}

impl<'a, T> SomeWhereCondition<'a> for WhereIn<T>
where
    T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
{
    fn apply_to(self: Box<Self>, query: &mut QueryBuilder<Postgres>) {
        if let Some(separator) = self.separator {
            query.push(separator);
        }

        if self.value.is_empty() {
            query.push("FALSE");
            return;
        }

        query.push(&self.column);
        query.push(SetOperator::In);
        query.push("(");
        let mut separated = query.separated(", ");
        for value in self.value {
            separated.push_bind(value);
        }
        query.push(")");
    }
}
