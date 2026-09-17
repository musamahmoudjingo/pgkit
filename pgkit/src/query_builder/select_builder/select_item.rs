use sqlx::{Postgres, QueryBuilder};

use crate::types::ColumnRef;

type ExprFn<'a> = Box<dyn FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a>;

/// One item in the `SELECT …` projection list.
pub(crate) enum SelectItem<'a> {
    /// `*`.
    All,
    /// `<qualifier>.*`, e.g. `c.*`.
    AllOf(String),
    /// A column reference: `name` or `c.name`.
    Column(ColumnRef),
    /// `<column> AS <alias>`.
    ColumnAs(ColumnRef, String),
    /// A raw SQL expression, e.g. `COUNT(*)`, `COALESCE(a, b)`.
    Expr(String),
    /// A raw SQL expression with alias: `<expr> AS <alias>`.
    ExprAs(String, String),
    /// A deferred expression that may push bound parameters at apply time.
    ExprWith(ExprFn<'a>),
    /// A deferred expression with alias: `<expr> AS <alias>`.
    ExprWithAs(ExprFn<'a>, String),
}

impl<'a> SelectItem<'a> {
    pub(crate) fn apply_to(self, query: &mut QueryBuilder<Postgres>) {
        match self {
            SelectItem::All => {
                query.push("*");
            }
            SelectItem::AllOf(qualifier) => {
                query.push(qualifier);
                query.push(".*");
            }
            SelectItem::Column(col) => {
                query.push(col.aliased());
            }
            SelectItem::ColumnAs(col, alias) => {
                query.push(&col);
                query.push(" AS ");
                query.push(alias);
            }
            SelectItem::Expr(sql) => {
                query.push(sql);
            }
            SelectItem::ExprAs(sql, alias) => {
                query.push(sql);
                query.push(" AS ");
                query.push(alias);
            }
            SelectItem::ExprWith(f) => {
                f(query);
            }
            SelectItem::ExprWithAs(f, alias) => {
                f(query);
                query.push(" AS ");
                query.push(alias);
            }
        }
    }
}
