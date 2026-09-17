use std::fmt;

use super::types::DatabaseTableColumn;

/// A set of columns to project, with the table's primary key guaranteed present.
///
/// [`new`](Self::new) auto-injects the primary key at index 0 when the caller
/// omits it, so a selection *always* contains the PK. Downstream consumers (in
/// particular cursor pagination, which needs the PK to mint and resolve
/// cursors) can rely on its presence without re-checking.
#[derive(Debug, Clone)]
pub struct ColumnSelection<I> {
    columns: Vec<I>,
}

impl<I> ColumnSelection<I>
where
    I: DatabaseTableColumn + PartialEq,
{
    /// Builds a selection, inserting the primary key at index 0 if the caller
    /// didn't include it.
    pub fn new(columns: impl IntoIterator<Item = I>) -> Self {
        let mut columns: Vec<I> = columns.into_iter().collect();
        let pk = I::table_primary_key();
        if !columns.contains(&pk) {
            columns.insert(0, pk);
        }
        Self { columns }
    }
}

impl<I: PartialEq> ColumnSelection<I> {
    pub fn contains(&self, column: &I) -> bool {
        self.columns.contains(column)
    }
}

impl<I> ColumnSelection<I> {
    pub fn as_slice(&self) -> &[I] {
        &self.columns
    }
}

/// Consume the column selection: `query.columns(selection)`.
impl<I> IntoIterator for ColumnSelection<I> {
    type Item = I;
    type IntoIter = std::vec::IntoIter<I>;

    fn into_iter(self) -> Self::IntoIter {
        self.columns.into_iter()
    }
}

/// Borrow the column selection: `query.columns(&selection)`. Clones each column.
impl<'a, I: Clone> IntoIterator for &'a ColumnSelection<I> {
    type Item = I;
    type IntoIter = std::iter::Cloned<std::slice::Iter<'a, I>>;

    fn into_iter(self) -> Self::IntoIter {
        self.columns.iter().cloned()
    }
}

impl<I> fmt::Display for ColumnSelection<I>
where
    I: DatabaseTableColumn,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut iter = self.columns.iter();
        if let Some(first) = iter.next() {
            f.write_str((*first).into())?;
            iter.try_for_each(|c| {
                f.write_str(", ")?;
                f.write_str((*c).into())
            })?;
        }
        Ok(())
    }
}
