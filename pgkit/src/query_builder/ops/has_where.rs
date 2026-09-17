use crate::query_builder::where_builder::WhereBuilder;

/// Implemented by query builders that embed a [`WhereBuilder`].
///
/// Provides the single accessor [`WhereOps`](super::WhereOps) needs to wire
/// the full WHERE-predicate surface into each builder via a blanket impl.
///
/// ```
/// # use pgkit::query_builder::{HasWhere, WhereBuilder, WhereOps};
/// struct MyBuilder<'a> {
///     where_clause: WhereBuilder<'a>,
/// }
///
/// impl<'a> HasWhere<'a> for MyBuilder<'a> {
///     fn where_mut(&mut self) -> &mut WhereBuilder<'a> {
///         &mut self.where_clause
///     }
/// }
///
/// // `MyBuilder` now has `.where_eq()`, `.where_in()`, `.group()`, etc.
/// let builder = MyBuilder { where_clause: WhereBuilder::new() }.where_eq("id", 1);
/// assert!(!builder.where_clause.is_empty());
/// ```
pub trait HasWhere<'a> {
    /// Mutable access to the builder's embedded WHERE-clause.
    fn where_mut(&mut self) -> &mut WhereBuilder<'a>;
}
