use sqlx::{Postgres, QueryBuilder};

use crate::types::ColumnRef;

/// The `RETURNING` clause for an INSERT (or UPDATE / DELETE).
///
/// Rendered as ` RETURNING <body>` when applied. Empty by default; call
/// the builder's `.returning_*` methods to populate.
#[derive(Default)]
pub enum Returning {
    /// No `RETURNING` clause.
    #[default]
    None,
    /// `RETURNING *`.
    All,
    /// `RETURNING col1, col2, …`.
    Columns(Vec<ColumnRef>),
    /// `RETURNING <raw-sql>`: escape hatch for expressions, casts, aliases.
    Raw(String),
}

impl Returning {
    pub(crate) fn apply_to(self, query: &mut QueryBuilder<Postgres>) {
        match self {
            Returning::None => {}
            Returning::All => {
                query.push(" RETURNING *");
            }
            Returning::Columns(cols) => {
                query.push(" RETURNING ");
                for (i, col) in cols.iter().enumerate() {
                    if i > 0 {
                        query.push(", ");
                    }
                    query.push(col.unqualified_aliased());
                }
            }
            Returning::Raw(sql) => {
                query.push(" RETURNING ");
                query.push(sql);
            }
        }
    }
}
