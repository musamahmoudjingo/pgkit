use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::path::Path;

use super::fs;
use super::walk::walk;

const SUFFIX: &str = ".up.sql";

/// Every migration under a root.
pub struct Migrations(Vec<Migration>);

struct Migration {
    version: i64,
    description: String,
    path: String,
    no_tx: bool,
}

impl Migrations {
    pub fn collect(root: &Path, span: Span) -> syn::Result<Migrations> {
        let mut migrations = Vec::new();
        for path in walk(root, "migrations", span)? {
            migrations.push(migration(&path, span)?);
        }

        migrations.sort_by_key(|m| m.version);
        // sqlx keys migrations by version: a duplicate would be silently
        // skipped at run time, so it fails the build instead.
        if let Some([a, b]) = migrations.windows(2).find(|w| w[0].version == w[1].version) {
            return Err(syn::Error::new(
                span,
                format!(
                    "migration version {} is used twice: {} and {}",
                    a.version, a.path, b.path
                ),
            ));
        }
        Ok(Migrations(migrations))
    }

    /// A `Vec<sqlx::migrate::Migration>` expression; the SQL is embedded with
    /// `include_str!`, which also makes Cargo re-run on content changes.
    pub fn render(&self) -> TokenStream {
        let items = self.0.iter().map(|m| {
            let Migration {
                version,
                description,
                path,
                no_tx,
            } = m;
            quote! {
                ::pgkit::db_init::migrating::migration(#version, #description, include_str!(#path), #no_tx)
            }
        });
        quote! { ::std::vec![ #(#items),* ] }
    }
}

fn migration(path: &Path, span: Span) -> syn::Result<Migration> {
    let name = fs::file_name(path, span)?;
    let (version, rest) = name
        .split_once('_')
        .filter(|(version, _)| version.len() == 14)
        .ok_or_else(|| fs::error(span, path, "expected `<14-digit version>_<name>.up.sql`"))?;
    let description = rest
        .strip_suffix(SUFFIX)
        .ok_or_else(|| fs::error(span, path, &format!("a migration must end in `{SUFFIX}`")))?;
    let version = version
        .parse()
        .map_err(|_| fs::error(span, path, "the version is not a number"))?;
    let sql = fs::read(path, span)?;
    Ok(Migration {
        version,
        // What `sqlx::migrate!` records: the name with `_` as spaces.
        description: description.replace('_', " "),
        path: fs::path_as_string(path, span)?,
        no_tx: sql.starts_with("-- no-transaction"),
    })
}
