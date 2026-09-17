//! Emits the column enum + its `strum` derives + `DatabaseTableColumn` impl.
//!
//! # What this emits
//!
//! For an input struct
//!
//! ```ignore
//! #[derive(PgKit)]
//! #[pgkit(table_name = "geo_countries")]
//! pub struct CountryRow {
//!     #[pgkit(primary_key)] pub id: i32,
//!     pub iso2: String,
//!     #[pgkit(rename = "deleted_at")] pub deleted: Option<DateTime<Utc>>,
//! }
//! ```
//!
//! this emits roughly (the `strum` / `serde` paths really resolve through
//! `::pgkit::__private`, shortened here for readability):
//!
//! ```ignore
//! #[derive(
//!     Copy, Clone, Debug, PartialEq, Eq, Hash, Default,
//!     strum::Display, strum::IntoStaticStr, strum::EnumString,
//!     strum::VariantNames, strum::EnumIter,
//!     serde::Serialize, serde::Deserialize,
//! )]
//! pub enum CountryColumn {
//!     #[default]
//!     #[strum(serialize = "id")]
//!     #[serde(rename = "id")]
//!     Id,
//!     #[strum(serialize = "iso2")]
//!     #[serde(rename = "iso2")]
//!     Iso2,
//!     #[strum(serialize = "deleted_at")]
//!     #[serde(rename = "deleted_at")]
//!     Deleted,
//! }
//!
//! impl ::pgkit::types::DatabaseTableColumn for CountryColumn {
//!     type Table = CountryTable;
//!     fn table_primary_key() -> Self { Self::Id }
//! }
//! ```
//!
//! Every variant pins its SQL column name with an explicit
//! `#[strum(serialize = "…")]` + `#[serde(rename = "…")]`: the field ident
//! verbatim, or the `#[pgkit(rename = "…")]` value. This keeps the
//! enum's rendered name identical to the name the partial's `FromRow` looks
//! up, instead of trusting a case conversion to round-trip the ident.

use proc_macro2::TokenStream;
use quote::quote;

use super::model::Model;

pub(crate) fn emit(model: &Model) -> TokenStream {
    // The column enum is the structural anchor; it's never skipped.
    // (skip(columns) is rejected at parse time.)
    let vis = &model.vis;
    let columns_ident = &model.names.columns;
    let table_ident = &model.names.table;
    let pk_variant = &model.primary_key_field().variant_ident;

    let variants = model.fields.iter().map(|f| {
        let variant = &f.variant_ident;
        let lit = &f.column_name;
        let default_attr = if f.variant_ident == *pk_variant {
            quote! { #[default] }
        } else {
            quote! {}
        };
        quote! {
            #default_attr
            #[strum(serialize = #lit)]
            #[serde(rename = #lit)]
            #variant
        }
    });

    quote! {
        #[derive(
            ::core::marker::Copy,
            ::core::clone::Clone,
            ::core::fmt::Debug,
            ::core::cmp::PartialEq,
            ::core::cmp::Eq,
            ::core::hash::Hash,
            ::core::default::Default,
            ::pgkit::__private::strum::Display,
            ::pgkit::__private::strum::IntoStaticStr,
            ::pgkit::__private::strum::EnumString,
            ::pgkit::__private::strum::VariantNames,
            ::pgkit::__private::strum::EnumIter,
            ::pgkit::__private::serde::Serialize,
            ::pgkit::__private::serde::Deserialize,
        )]
        // Point the strum/serde expansions at pgkit's re-exports so callers
        // don't need either crate as a direct dependency.
        #[strum(crate = "::pgkit::__private::strum")]
        #[serde(crate = "::pgkit::__private::serde")]
        #[allow(dead_code)]
        #vis enum #columns_ident {
            #(#variants),*
        }

        #[automatically_derived]
        impl ::pgkit::types::DatabaseTableColumn for #columns_ident {
            type Table = #table_ident;
            fn table_primary_key() -> Self {
                Self::#pk_variant
            }
        }
    }
}
