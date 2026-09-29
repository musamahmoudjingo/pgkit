use sqlx::AssertSqlSafe;

use super::error::SeedError;
use super::errors::SeedErrors;
use super::seed::Seed;

/// Applies seeds.
///
/// The seeder tracks nothing, every script executes on every run.
/// Scripts must guard against data corruption on their own.
///
/// Two common guards:
///
/// Insert only when the table is still empty (new database or table):
///
/// ```sql
/// INSERT INTO plans (code, name)
/// SELECT code, name FROM (VALUES
///     ('standard', 'Standard'),
///     ('premium', 'Premium')
/// ) AS seed (code, name)
/// WHERE NOT EXISTS (SELECT 1 FROM plans);
/// ```
///
/// Or insert per row, skipping rows that already exist (never overwrites,
/// so edits to existing rows are kept):
///
/// ```sql
/// INSERT INTO plans (code, name)
/// VALUES ('standard', 'Standard'), ('premium', 'Premium')
/// ON CONFLICT (code) DO NOTHING;
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Seeder {
    seeds: &'static [Seed],
}

impl Seeder {
    pub const fn new(seeds: &'static [Seed]) -> Self {
        Seeder { seeds }
    }

    pub const fn seeds(&self) -> &'static [Seed] {
        self.seeds
    }

    /// Run the seed scripts on the database.
    pub async fn run(&self, pool: &sqlx::PgPool) -> Result<(), SeedErrors> {
        let mut errors = Vec::new();
        for seed in self.seeds {
            if let Err(source) = sqlx::raw_sql(AssertSqlSafe(seed.sql)).execute(pool).await {
                tracing::error!(seed = seed.path, error = %source, "seed failed");
                errors.push(SeedError {
                    path: seed.path,
                    source,
                });
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(SeedErrors(errors))
        }
    }
}
