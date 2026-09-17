use sqlx::{Postgres, QueryBuilder};

use crate::types::ColumnRef;

/// `DISTINCT` modifier on the SELECT list.
#[derive(Default)]
pub enum Distinct {
    /// Plain SELECT (default).
    #[default]
    None,
    /// `SELECT DISTINCT`.
    All,
    /// `SELECT DISTINCT ON (col1, col2, …)`.
    On(Vec<ColumnRef>),
}

impl Distinct {
    pub(crate) fn apply_to(&self, query: &mut QueryBuilder<Postgres>) {
        match self {
            Distinct::None => {}
            Distinct::All => {
                query.push("DISTINCT ");
            }
            Distinct::On(cols) => {
                query.push("DISTINCT ON (");
                for (i, c) in cols.iter().enumerate() {
                    if i > 0 {
                        query.push(", ");
                    }
                    query.push(c);
                }
                query.push(") ");
            }
        }
    }
}
