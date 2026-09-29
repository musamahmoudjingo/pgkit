use std::fmt;

use super::error::SeedError;

/// Every failure from single `Seeder::run`.
#[derive(Debug)]
pub struct SeedErrors(pub Vec<SeedError>);

impl fmt::Display for SeedErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} seed script(s) failed", self.0.len())?;
        for error in &self.0 {
            write!(f, "; {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for SeedErrors {}
