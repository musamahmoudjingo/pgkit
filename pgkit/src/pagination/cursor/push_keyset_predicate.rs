use sqlx::postgres::PgTypeInfo;
use sqlx::{Encode, Postgres, QueryBuilder, Type, TypeInfo};

use crate::ordering::OrderByDirection;
use crate::pagination::cursor::CursorValue;
use crate::types::ColumnRef;

/// Push the keyset predicate body (wrapped in `(...)`) into `sqlx::QueryBuilder`.
///
/// `cast` is the Postgres type the sort value must be cast to when it is a
/// [`CursorValue::Enum`], recovered server-side from the ordering column (see
/// [`crate::types::ColumnTypeInfo`]), never from the cursor. It is `None` for
/// scalar sort columns, where the value binds directly.
pub(crate) fn push_keyset_predicate<'a, Id>(
    qb: &mut QueryBuilder<Postgres>,
    col: &ColumnRef,
    id_col: &ColumnRef,
    dir: OrderByDirection,
    val: Option<CursorValue>,
    id: Id,
    cast: Option<PgTypeInfo>,
) where
    Id: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
{
    use OrderByDirection::{Asc, Desc};
    qb.push("(");
    match (dir, val) {
        (Asc, Some(v)) => {
            qb.push(col).push(" > ");
            push_val(qb, &v, cast.as_ref());
            qb.push(" OR (").push(col).push(" = ");
            push_val(qb, &v, cast.as_ref());
            qb.push(" AND ").push(id_col).push(" > ");
            qb.push_bind(id);
            qb.push(") OR ").push(col).push(" IS NULL");
        }
        (Asc, None) => {
            qb.push(col).push(" IS NULL AND ").push(id_col).push(" > ");
            qb.push_bind(id);
        }
        (Desc, Some(v)) => {
            qb.push(col).push(" < ");
            push_val(qb, &v, cast.as_ref());
            qb.push(" OR (").push(col).push(" = ");
            push_val(qb, &v, cast.as_ref());
            qb.push(" AND ").push(id_col).push(" < ");
            qb.push_bind(id);
            qb.push(")");
        }
        (Desc, None) => {
            qb.push("(")
                .push(col)
                .push(" IS NULL AND ")
                .push(id_col)
                .push(" < ");
            qb.push_bind(id);
            qb.push(") OR ").push(col).push(" IS NOT NULL");
        }
    }
    qb.push(")");
}

/// Push a single keyset value. An enum label is bound and cast to the column's
/// Postgres type; every other value binds directly.
fn push_val(qb: &mut QueryBuilder<Postgres>, v: &CursorValue, cast: Option<&PgTypeInfo>) {
    match (v, cast) {
        (CursorValue::Enum(label), Some(ti)) => {
            qb.push("CAST(");
            qb.push_bind(label.clone());
            qb.push(" AS ");
            qb.push(ti.name());
            qb.push(")");
        }
        (scalar, _) => scalar.clone().push_bind_to(qb),
    }
}
