use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use std::path::PathBuf;
use syn::LitStr;

use super::migration::Migrations;
use super::seed::Seeds;

pub fn migrations(input: TokenStream) -> TokenStream {
    expand(input, |root, span| {
        Migrations::collect(root, span).map(|m| m.render())
    })
}

pub fn migrator(input: TokenStream) -> TokenStream {
    expand(input, |root, span| {
        let vec = Migrations::collect(root, span)?.render();
        Ok(quote! { ::pgkit::db_init::migrating::migrator(#vec) })
    })
}

pub fn seeds(input: TokenStream) -> TokenStream {
    expand(input, |root, span| {
        Seeds::collect(root, span).map(|s| s.render())
    })
}

pub fn seeder(input: TokenStream) -> TokenStream {
    expand(input, |root, span| {
        let slice = Seeds::collect(root, span)?.render();
        Ok(quote! { ::pgkit::db_init::seeding::Seeder::new(#slice) })
    })
}

fn expand(
    input: TokenStream,
    render: impl FnOnce(&std::path::Path, Span) -> syn::Result<TokenStream2>,
) -> TokenStream {
    root(input)
        .and_then(|(root, span)| render(&root, span))
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

/// The scan root: the macro's one string-literal argument, resolved against
/// the calling crate's manifest directory.
fn root(input: TokenStream) -> syn::Result<(PathBuf, Span)> {
    let lit: LitStr = syn::parse(input)?;
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| syn::Error::new(lit.span(), "CARGO_MANIFEST_DIR is not set"))?;
    Ok((PathBuf::from(manifest_dir).join(lit.value()), lit.span()))
}
