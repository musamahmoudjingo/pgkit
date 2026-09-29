use sqlx::migrate::{Migration, Migrator};

/// A ready-to-run migrator. `pgkit::migrator!` expands to a call of this.
pub fn migrator(migrations: Vec<Migration>) -> Migrator {
    Migrator::with_migrations(migrations)
}
