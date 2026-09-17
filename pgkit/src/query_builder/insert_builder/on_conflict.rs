use sqlx::{Encode, Postgres, QueryBuilder, Type};

use super::builder::InsertBuilder;
use super::row::Binder;
use crate::types::ColumnRef;

/// What target an `ON CONFLICT` clause uses to detect a collision.
pub enum ConflictTarget {
    /// `ON CONFLICT` (no target; relies on any unique constraint matching).
    Any,
    /// `ON CONFLICT (col1, col2, …)`.
    Columns(Vec<ColumnRef>),
    /// `ON CONFLICT ON CONSTRAINT <name>`.
    Constraint(String),
}

impl ConflictTarget {
    pub(crate) fn apply_to(&self, query: &mut QueryBuilder<Postgres>) {
        match self {
            ConflictTarget::Any => {}
            ConflictTarget::Columns(cols) => {
                query.push(" (");
                for (i, col) in cols.iter().enumerate() {
                    if i > 0 {
                        query.push(", ");
                    }
                    query.push(col.unqualified());
                }
                query.push(")");
            }
            ConflictTarget::Constraint(name) => {
                query.push(" ON CONSTRAINT ");
                query.push(name);
            }
        }
    }
}

/// An individual `SET col = expr` assignment used by `DO UPDATE`.
pub(crate) enum UpdateAssignment<'a> {
    /// `SET col = EXCLUDED.col`.
    Excluded(ColumnRef),
    /// `SET col = $N` (parameter-bound).
    Bound { column: ColumnRef, bind: Binder<'a> },
    /// `SET col = <raw expression>`.
    Raw { column: ColumnRef, expr: Binder<'a> },
}

/// The action of an `ON CONFLICT` clause: `DO NOTHING` or `DO UPDATE SET …`.
pub(crate) enum ConflictAction<'a> {
    Nothing,
    Update(Vec<UpdateAssignment<'a>>),
}

/// `ON CONFLICT (...) DO …` clause, attached to an [`InsertBuilder`].
pub(crate) struct OnConflictClause<'a> {
    pub(crate) target: ConflictTarget,
    pub(crate) action: ConflictAction<'a>,
}

impl<'a> OnConflictClause<'a> {
    pub(crate) fn apply_to(self, query: &mut QueryBuilder<Postgres>) {
        query.push(" ON CONFLICT");
        self.target.apply_to(query);
        match self.action {
            ConflictAction::Nothing => {
                query.push(" DO NOTHING");
            }
            ConflictAction::Update(assignments) => {
                query.push(" DO UPDATE SET ");
                for (i, a) in assignments.into_iter().enumerate() {
                    if i > 0 {
                        query.push(", ");
                    }
                    match a {
                        UpdateAssignment::Excluded(col) => {
                            query.push(col.unqualified());
                            query.push(" = EXCLUDED.");
                            // Print the column's unqualified name on the EXCLUDED side.
                            match &col {
                                ColumnRef::Bare(name) => query.push(name.as_ref()),
                                ColumnRef::Qualified { column, .. }
                                | ColumnRef::Aliased { column, .. } => query.push(*column),
                            };
                        }
                        UpdateAssignment::Bound { column, bind } => {
                            query.push(column.unqualified());
                            query.push(" = ");
                            bind(query);
                        }
                        UpdateAssignment::Raw { column, expr } => {
                            query.push(column.unqualified());
                            query.push(" = ");
                            expr(query);
                        }
                    }
                }
            }
        }
    }
}

/// Intermediate returned by [`InsertBuilder::on_conflict`] (and its variants).
///
/// Call [`do_nothing`] or [`do_update`] to finalize and hand control back to
/// the parent [`InsertBuilder`].
///
/// [`do_nothing`]: Self::do_nothing
/// [`do_update`]: Self::do_update
pub struct OnConflict<'a> {
    pub(crate) insert: InsertBuilder<'a>,
    pub(crate) target: ConflictTarget,
}

impl<'a> OnConflict<'a> {
    /// `ON CONFLICT … DO NOTHING`.
    pub fn do_nothing(mut self) -> InsertBuilder<'a> {
        self.insert.on_conflict = Some(OnConflictClause {
            target: self.target,
            action: ConflictAction::Nothing,
        });
        self.insert
    }

    /// `ON CONFLICT … DO UPDATE SET …`.
    ///
    /// The closure receives a fresh [`OnConflictUpdate`] for declaring
    /// the SET assignments.
    ///
    /// ```
    /// # use pgkit::query_builder::Query;
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "auth_permissions")] struct PermissionsRow { #[pgkit(primary_key)] id: i32, code: String, name: String, description: String }
    /// # use PermissionsColumn::*;
    /// let query = Query::insert().into(PermissionsTable)
    ///     .value(Code, "users.read")
    ///     .value(Name, "Read users")
    ///     .value(Description, "Can view users")
    ///     .on_conflict([Code]).do_update(|u| u
    ///         .set_excluded(Name)
    ///         .set_excluded(Description))
    ///     .build();
    /// assert_eq!(
    ///     query.sql(),
    ///     "INSERT INTO auth_permissions (code, name, description) VALUES ($1, $2, $3) ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name, description = EXCLUDED.description"
    /// );
    /// ```
    pub fn do_update<F>(mut self, callback: F) -> InsertBuilder<'a>
    where
        F: FnOnce(OnConflictUpdate<'a>) -> OnConflictUpdate<'a>,
    {
        let upd = callback(OnConflictUpdate::new());
        self.insert.on_conflict = Some(OnConflictClause {
            target: self.target,
            action: ConflictAction::Update(upd.assignments),
        });
        self.insert
    }
}

/// Builder for the `SET …` assignments inside `DO UPDATE`.
pub struct OnConflictUpdate<'a> {
    assignments: Vec<UpdateAssignment<'a>>,
}

impl<'a> OnConflictUpdate<'a> {
    pub(crate) fn new() -> Self {
        Self {
            assignments: Vec::new(),
        }
    }

    /// `SET col = $N`.
    pub fn set<C, T>(mut self, column: C, value: T) -> Self
    where
        C: Into<ColumnRef>,
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.assignments.push(UpdateAssignment::Bound {
            column: column.into(),
            bind: Box::new(move |q| {
                q.push_bind(value);
            }),
        });
        self
    }

    /// `SET col = EXCLUDED.col`: copy the proposed-insert value.
    pub fn set_excluded<C: Into<ColumnRef>>(mut self, column: C) -> Self {
        self.assignments
            .push(UpdateAssignment::Excluded(column.into()));
        self
    }

    /// `SET col = <raw expression>`: for things like `NOW()`,
    /// `col + EXCLUDED.col`, casts, etc.
    pub fn set_raw<C, F>(mut self, column: C, expr: F) -> Self
    where
        C: Into<ColumnRef>,
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.assignments.push(UpdateAssignment::Raw {
            column: column.into(),
            expr: Box::new(expr),
        });
        self
    }
}

impl<'a> Default for OnConflictUpdate<'a> {
    fn default() -> Self {
        Self::new()
    }
}
