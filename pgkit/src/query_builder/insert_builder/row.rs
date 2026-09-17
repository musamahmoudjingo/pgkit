use sqlx::{Encode, Postgres, QueryBuilder, Type};

/// Boxed closure that pushes a single bound value into a [`QueryBuilder`].
pub(crate) type Binder<'a> = Box<dyn FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a>;

/// A single row of bound values for a multi-row INSERT.
///
/// Used inside the closure passed to [`InsertBuilder::row`](super::InsertBuilder::row).
///
/// ```
/// # use pgkit::query_builder::InsertBuilder;
/// let query = InsertBuilder::new().into("t").columns(["a", "b"])
///     .row(|r| r.bind(1).bind("x"))
///     .row(|r| r.bind(2).bind("y"))
///     .build();
/// assert_eq!(query.sql(), "INSERT INTO t (a, b) VALUES ($1, $2), ($3, $4)");
/// ```
pub struct InsertRow<'a> {
    pub(crate) binds: Vec<Binder<'a>>,
}

impl<'a> InsertRow<'a> {
    pub(crate) fn new() -> Self {
        Self { binds: Vec::new() }
    }

    /// Bind one value for this row. Order must match the column list.
    pub fn bind<T>(mut self, value: T) -> Self
    where
        T: Encode<'a, Postgres> + Type<Postgres> + Send + 'a,
    {
        self.binds.push(Box::new(move |q| {
            q.push_bind(value);
        }));
        self
    }

    /// Bind a raw expression for this row, e.g. `r.bind_raw(|q| q.push("NOW()"))`.
    pub fn bind_raw<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(&mut QueryBuilder<Postgres>) + Send + 'a,
    {
        self.binds.push(Box::new(callback));
        self
    }

    /// Number of bound values pushed so far.
    pub fn len(&self) -> usize {
        self.binds.len()
    }

    /// Whether no values have been bound yet.
    pub fn is_empty(&self) -> bool {
        self.binds.is_empty()
    }
}

impl<'a> Default for InsertRow<'a> {
    fn default() -> Self {
        Self::new()
    }
}
