//! Database initialization.
//!
//! - [`migrating`] — builds `sqlx::migrate::Migration`s the way
//!   `sqlx::migrate!` does, so `_sqlx_migrations` checksums match.
//! - [`seeding`] — [`seeding::Seeder`] runs [`seeding::Seed`]s.
//!
//! ```rust,ignore
//! let pool = sqlx::PgPool::connect(&database_url).await?;
//!
//! pgkit::migrator!("src").run(&pool).await?; // an sqlx::migrate::Migrator
//! pgkit::seeder!("src").run(&pool).await?;   // a seeding::Seeder
//! ```

pub mod migrating;
pub mod seeding;
