use sqlx::{Postgres, QueryBuilder};

use super::nulls_order::NullsOrder;
use crate::ordering::OrderByDirection;
use crate::types::ColumnRef;

/// One item in the `ORDER BY` list.
pub enum OrderItem {
    Column {
        col: ColumnRef,
        dir: OrderByDirection,
        nulls: Option<NullsOrder>,
    },
    Raw(String),
}

impl OrderItem {
    pub(crate) fn apply_to(self, query: &mut QueryBuilder<Postgres>) {
        match self {
            OrderItem::Column { col, dir, nulls } => {
                query.push(&col);
                query.push(" ");
                query.push(dir);
                if let Some(n) = nulls {
                    query.push(" ");
                    query.push(n);
                }
            }
            OrderItem::Raw(sql) => {
                query.push(sql);
            }
        }
    }
}
