use std::fmt;

/// Error that could happen during seeding
#[derive(Debug)]
pub struct SeedError {
    pub path: &'static str,
    pub source: sqlx::Error,
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error executing seed {}: {}", self.path, self.source)
    }
}

impl std::error::Error for SeedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
