//! `migrations!` / `migrator!` / `seeds!` / `seeder!`: compile-time discovery
//! of a project's SQL. The scan root is walked recursively; a `.sql` file
//! directly inside a folder named `migrations` is a migration, inside `seeds`
//! a seed. The layout around those folders is the consumer's business.
//!
//! Each macro takes one string literal: the scan root, relative to the calling
//! crate's `CARGO_MANIFEST_DIR`. The expansion embeds every file with
//! `include_str!`, so the binary carries its SQL and needs no files at runtime.
//!
//! File rules, checked here so a bad file fails the build, not a deploy:
//! - migration: `<14-digit version>_<name>.up.sql`, versions unique across the tree
//! - seed: any name; files run in path order (zero-padded number prefixes
//!   are recommended to control it, but not enforced)

mod expand;
mod fs;
mod migration;
mod seed;
mod walk;

pub(crate) use expand::{migrations, migrator, seeder, seeds};
