use sqlx::{Postgres, QueryBuilder};

use super::builder::SelectBuilder;

type ExprFn<'a> = Box<dyn FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a>;

/// Body of a single CTE: either a (sub-)select or an arbitrary deferred SQL fragment.
pub(crate) enum CteBody<'a> {
    Select(Box<SelectBuilder<'a>>),
    Raw(ExprFn<'a>),
}

/// One `<name> AS (<body>)` clause inside `WITH …`.
pub(crate) struct CteItem<'a> {
    pub(crate) name: String,
    pub(crate) recursive: bool,
    pub(crate) body: CteBody<'a>,
}

impl<'a> CteItem<'a> {
    pub(crate) fn apply_to(self, query: &mut QueryBuilder<Postgres>) {
        query.push(&self.name);
        query.push(" AS (");
        match self.body {
            CteBody::Select(s) => s.apply_to(query),
            CteBody::Raw(f) => f(query),
        }
        query.push(")");
    }
}

/// Construction helpers re-exported as `CteBuilder` so users have a simple
/// type to write against. Currently just a marker; CTEs are added via
/// [`SelectBuilder::with_cte`](super::SelectBuilder::with_cte) and
/// [`SelectBuilder::with_recursive`](super::SelectBuilder::with_recursive).
pub struct CteBuilder;
