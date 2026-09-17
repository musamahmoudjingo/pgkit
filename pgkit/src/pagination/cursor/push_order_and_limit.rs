use sqlx::{Postgres, QueryBuilder};

use crate::pagination::cursor::CursorPagination;
use crate::query_builder::NullsOrder;
use crate::types::{ColumnRef, DatabaseTableColumn};

/// Push ` ORDER BY col dir NULLS …, id_col dir NULLS … LIMIT $n` into `qb`.
pub(super) fn push_order_and_limit<T, Id>(
    qb: &mut QueryBuilder<Postgres>,
    pagination: &CursorPagination<T, Id>,
    id_col: &ColumnRef,
) where
    T: DatabaseTableColumn + Copy,
{
    let (col, dir) = pagination.order_by();
    let col_ref = ColumnRef::from(col);
    let nulls = NullsOrder::from(dir);

    qb.push(" ORDER BY ")
        .push(&col_ref)
        .push(format_args!(" {dir} {nulls}, "))
        .push(id_col)
        .push(format_args!(" {dir} {nulls} LIMIT "))
        .push_bind(pagination.fetch_limit());
}
