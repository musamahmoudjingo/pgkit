//! Emits the table unit struct + its `DatabaseTable` impl.
//!
//! # What this emits
//!
//! For
//!
//! ```ignore
//! #[derive(PgKit)]
//! #[pgkit(table_name = "geo_countries")]
//! pub struct CountryRow { #[pgkit(primary_key)] pub id: i32, /* … */ }
//! ```
//!
//! this emits roughly:
//!
//! ```ignore
//! #[derive(Copy, Clone, Debug)]
//! pub struct CountryTable;
//!
//! impl ::pgkit::types::DatabaseTable for CountryTable {
//!     type Columns = CountryColumn;
//!     const NAME: &'static str = "geo_countries";
//!     fn primary_key() -> Self::Columns { CountryColumn::Id }
//! }
//! ```
//!
//! The SQL table name lives in the `DatabaseTable::NAME` associated
//! constant, its single home. `DatabaseTableColumn::qualified` reads it
//! back to render a table-prefixed column reference
//! (`geo_countries.name`), so the column enum never carries the name
//! itself. The struct is a bare type marker; nothing depends on it being
//! constructible.
//!
//! Always emitted: the table struct is derived entirely from `table_name`,
//! and the column enum's `DatabaseTableColumn` impl names it; there is no
//! `skip(table)`.

use proc_macro2::TokenStream;
use quote::quote;

use super::model::Model;

pub(crate) fn emit(model: &Model) -> TokenStream {
    let vis = &model.vis;
    let table_ident = &model.names.table;
    let columns_ident = &model.names.columns;
    let pk_variant = &model.primary_key_field().variant_ident;
    let table_name = &model.table_name;

    // Override `soft_delete_column` when a `deleted_at` column is present.
    let soft_delete_fn = model
        .fields
        .iter()
        .find(|f| f.column_name == "deleted_at")
        .map(|f| {
            let variant = &f.variant_ident;
            quote! {
                fn soft_delete_column() -> ::core::option::Option<Self::Columns> {
                    ::core::option::Option::Some(#columns_ident::#variant)
                }
            }
        })
        .unwrap_or_default();

    quote! {
        #[derive(
            ::core::marker::Copy,
            ::core::clone::Clone,
            ::core::fmt::Debug,
        )]
        #vis struct #table_ident;

        #[automatically_derived]
        impl ::pgkit::types::DatabaseTable for #table_ident {
            type Columns = #columns_ident;
            const NAME: &'static str = #table_name;
            fn primary_key() -> Self::Columns {
                #columns_ident::#pk_variant
            }
            #soft_delete_fn
        }
    }
}
