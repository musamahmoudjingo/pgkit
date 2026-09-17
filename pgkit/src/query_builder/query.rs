use super::delete_builder::DeleteBuilder;
use super::insert_builder::InsertBuilder;
use super::select_builder::SelectBuilder;
use super::update_builder::UpdateBuilder;

/// Convenient entry point for building SQL queries.
///
/// ```
/// use pgkit::query_builder::{Query, WhereOps};
///
/// let select = Query::select().from("t").columns(["id"]).where_eq("id", 1).build();
/// assert_eq!(select.sql(), "SELECT id FROM t WHERE id = $1");
///
/// let insert = Query::insert().into("t").value("id", 1).returning_all().build();
/// assert_eq!(insert.sql(), "INSERT INTO t (id) VALUES ($1) RETURNING *");
///
/// let update = Query::update().table("t").set("v", 2).where_eq("id", 1).build();
/// assert_eq!(update.sql(), "UPDATE t SET v = $1 WHERE id = $2");
///
/// let delete = Query::delete().from("t").where_eq("id", 1).build();
/// assert_eq!(delete.sql(), "DELETE FROM t WHERE id = $1");
/// ```
///
/// All four builders return a [`sqlx::QueryBuilder`] from their `.build()`
/// terminal; chain sqlx's `.build_query_as::<T>()` / `.fetch_one(&pool)` /
/// etc.
pub struct Query;

impl Query {
    /// Start a `SELECT …` statement. Chain `.from(table)` to set the source.
    pub fn select<'a>() -> SelectBuilder<'a> {
        SelectBuilder::new()
    }

    /// Start an `INSERT …` statement. Chain `.into(table)` to set the target.
    pub fn insert<'a>() -> InsertBuilder<'a> {
        InsertBuilder::new()
    }

    /// Start an `UPDATE …` statement. Chain `.table(table)` to set the target.
    pub fn update<'a>() -> UpdateBuilder<'a> {
        UpdateBuilder::new()
    }

    /// Start a `DELETE …` statement. Chain `.from(table)` to set the source.
    pub fn delete<'a>() -> DeleteBuilder<'a> {
        DeleteBuilder::new()
    }
}
