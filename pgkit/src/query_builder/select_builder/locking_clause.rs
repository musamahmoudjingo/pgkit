use sqlx::{Postgres, QueryBuilder};

use super::locking::Locking;
use crate::types::TableRef;

pub(super) struct LockingClause {
    pub(super) kind: Locking,
    pub(super) of: Vec<TableRef>,
    pub(super) skip_locked: bool,
    pub(super) nowait: bool,
}

impl LockingClause {
    pub(super) fn apply_to(&self, query: &mut QueryBuilder<Postgres>) {
        query.push(self.kind.keyword());
        if !self.of.is_empty() {
            query.push(" OF ");
            for (i, t) in self.of.iter().enumerate() {
                if i > 0 {
                    query.push(", ");
                }
                // Postgres resolves `OF` against the FROM-clause alias when
                // the table is aliased, not the underlying table name.
                query.push(t.alias());
            }
        }
        if self.nowait {
            query.push(" NOWAIT");
        }
        if self.skip_locked {
            query.push(" SKIP LOCKED");
        }
    }
}
