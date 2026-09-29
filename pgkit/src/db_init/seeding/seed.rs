/// A single seed script.
#[derive(Debug, Clone, Copy)]
pub struct Seed {
    /// Path to the seed file
    pub path: &'static str,
    /// Content (sql) of the seed file
    pub sql: &'static str,
}
