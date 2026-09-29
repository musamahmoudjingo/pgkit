use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::path::Path;

use super::fs;
use super::walk::walk;

/// Every seed under a scan root.
pub struct Seeds(Vec<Seed>);

struct Seed {
    /// Absolute path, for `include_str!`.
    path: String,
    /// Path relative to the root: what the runtime `Seed::path` carries.
    label: String,
}

impl Seeds {
    pub fn collect(root: &Path, span: Span) -> syn::Result<Seeds> {
        walk(root, "seeds", span)?
            .iter()
            .map(|path| {
                Ok(Seed {
                    path: fs::path_as_string(path, span)?,
                    label: fs::relative(root, path, span)?,
                })
            })
            .collect::<syn::Result<_>>()
            .map(Seeds)
    }

    pub fn render(&self) -> TokenStream {
        let items = self.0.iter().map(|s| {
            let Seed { path, label } = s;
            quote! {
                ::pgkit::db_init::seeding::Seed { path: #label, sql: include_str!(#path) }
            }
        });
        quote! { &[ #(#items),* ] }
    }
}
