use std::borrow::Cow;

use sqlx::SqlStr;
use sqlx::migrate::{Migration, MigrationType};

/// Build a single [`Migration`]
pub fn migration(
    version: i64,
    description: &'static str,
    sql: &'static str,
    no_tx: bool,
) -> Migration {
    Migration::new(
        version,
        Cow::Borrowed(description),
        MigrationType::ReversibleUp,
        SqlStr::from_static(sql),
        no_tx,
    )
}
