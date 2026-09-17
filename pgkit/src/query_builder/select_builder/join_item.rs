use sqlx::{Postgres, QueryBuilder};

use super::join_kind::JoinKind;
use super::join_on::JoinOn;
use crate::types::TableRef;

/// A single JOIN clause attached to a SELECT.
pub(crate) struct JoinItem<'a> {
    pub(crate) kind: JoinKind,
    pub(crate) table: TableRef,
    /// `CROSS JOIN` has no `ON`; everything else does.
    pub(crate) on: Option<JoinOn<'a>>,
}

impl<'a> JoinItem<'a> {
    pub(crate) fn apply_to(self, query: &mut QueryBuilder<Postgres>) {
        query.push(" ");
        query.push(self.kind.keyword());
        query.push(" ");
        query.push(&self.table);
        if let Some(on) = self.on {
            query.push(" ON ");
            on.inner.apply_predicate(query);
        }
    }
}
