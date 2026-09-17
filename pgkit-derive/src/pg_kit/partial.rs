//! Emits the partial row struct + its hand-rolled `sqlx::FromRow<PgRow>`.
//!
//! # What this emits
//!
//! For
//!
//! ```ignore
//! #[derive(PgKit)]
//! #[pgkit(table_name = "geo_countries")]
//! pub struct CountryRow {
//!     #[pgkit(primary_key)] pub id: i32,
//!     pub iso2: String,
//!     pub native: Option<String>,
//!     #[pgkit(rename = "deleted_at")] pub deleted: Option<DateTime<Utc>>,
//! }
//! ```
//!
//! this emits roughly:
//!
//! ```ignore
//! #[derive(Debug, Clone, Default)]
//! pub struct PartialCountryRow {
//!     pub id: Option<i32>,
//!     pub iso2: Option<String>,
//!     pub native: Option<Option<String>>,
//!     pub deleted: Option<Option<DateTime<Utc>>>,
//! }
//!
//! impl<'r> sqlx::FromRow<'r, PgRow> for PartialCountryRow {
//!     fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
//!         Ok(Self {
//!             id:      pgkit::try_get_option::<i32>(row, "id")?,
//!             iso2:    pgkit::try_get_option::<String>(row, "iso2")?,
//!             native:  pgkit::try_get_option::<Option<String>>(row, "native")?,
//!             deleted: pgkit::try_get_option::<Option<DateTime<Utc>>>(row, "deleted_at")?,
//!         })
//!     }
//! }
//! ```
//!
//! The hand-rolled `FromRow` calls `pgkit::try_get_option`, which silently
//! returns `None` for columns missing from the projection, exactly what
//! makes the partial usable for queries that select a caller-chosen subset
//! of columns.
//!
//! Skipped entirely when the user opts out via
//! `#[pgkit(skip(partial))]`.

use proc_macro2::TokenStream;
use quote::quote;

use super::model::Model;

pub(crate) fn emit(model: &Model) -> TokenStream {
    if model.skip.partial {
        return TokenStream::new();
    }

    let vis = &model.vis;
    let partial_ident = &model.names.partial;
    let table_name = &model.table_name;

    // Parallel lists: one entry per field. Keeping them parallel avoids
    // two passes and matches the convention used in `partial_row/derive.rs`.
    let mut field_decls: Vec<TokenStream> = Vec::with_capacity(model.fields.len());
    let mut field_inits: Vec<TokenStream> = Vec::with_capacity(model.fields.len());
    let mut field_inits_prefixed: Vec<TokenStream> = Vec::with_capacity(model.fields.len());
    let mut alias_guards: Vec<TokenStream> = Vec::new();

    for field in &model.fields {
        let ident = &field.ident;
        let vis = &field.vis;
        let column = &field.column_name;

        // PostgreSQL silently truncates identifiers over 63 bytes, so an
        // overlong `{table}_{column}` embed alias would make
        // `from_included_columns` silently decode nothing at runtime.
        let alias_len = table_name.len() + 1 + column.len();
        if alias_len > 63 {
            let msg = format!(
                "embed alias `{table_name}_{column}` is {alias_len} bytes, over PostgreSQL's \
                 63-byte identifier limit; the truncated alias would make \
                 `from_included_columns` silently decode nothing; shorten the table or \
                 column name",
            );
            alias_guards.push(syn::Error::new(field.ident.span(), msg).to_compile_error());
        }

        // Field-type rule (mirrors `partial_row`):
        //   T         → Option<T>,        decoded as T
        //   Option<U> → Option<Option<U>>, decoded as Option<U>
        let (declared_ty, decode_ty) = if let Some(inner) = &field.option_inner {
            (
                quote! { ::core::option::Option<::core::option::Option<#inner>> },
                quote! { ::core::option::Option<#inner> },
            )
        } else {
            let ty = &field.ty;
            (quote! { ::core::option::Option<#ty> }, quote! { #ty })
        };

        field_decls.push(quote! { #vis #ident: #declared_ty });
        field_inits.push(quote! {
            #ident: ::pgkit::try_get_option::<#decode_ty>(row, #column)?
        });
        field_inits_prefixed.push(quote! {
            #ident: ::pgkit::try_get_option::<#decode_ty>(
                row,
                ::core::concat!(#table_name, "_", #column),
            )?
        });
    }

    // Primary key: used by `from_included_columns` to detect whether the embedded
    // resource's columns are present (and non-NULL for a LEFT join).
    let pk_field = model.primary_key_field();
    let pk_column = &pk_field.column_name;
    let pk_decode_ty = if let Some(inner) = &pk_field.option_inner {
        quote! { ::core::option::Option<#inner> }
    } else {
        let ty = &pk_field.ty;
        quote! { #ty }
    };

    // The `'__pgkit_r` lifetime name is intentionally
    // long-prefixed so it can't shadow a lifetime the user introduced in
    // scope (same trick as `partial_row/derive.rs`'s `'__partial_row_r`).
    quote! {
        #(#alias_guards)*

        #[derive(::core::fmt::Debug, ::core::clone::Clone, ::core::default::Default)]
        #[allow(dead_code)]
        #vis struct #partial_ident {
            #(#field_decls,)*
        }

        #[automatically_derived]
        impl<'__pgkit_r> ::pgkit::__private::sqlx::FromRow<'__pgkit_r, ::pgkit::__private::sqlx::postgres::PgRow>
            for #partial_ident
        {
            fn from_row(
                row: &'__pgkit_r ::pgkit::__private::sqlx::postgres::PgRow,
            ) -> ::pgkit::__private::sqlx::Result<Self> {
                ::core::result::Result::Ok(Self {
                    #(#field_inits,)*
                })
            }
        }

        #[automatically_derived]
        #[allow(dead_code)]
        impl #partial_ident {
            /// Reads an embedded resource from `<table_name>_<column>`-aliased
            /// columns keyed on its primary key:
            /// absent → `None`,
            /// present-but-NULL → `Some(None)`,
            /// matched → `Some(Some(_))`.
            ///
            /// See [`SelectBuilder::include`](pgkit::query_builder::SelectBuilder::include).
            pub fn from_included_columns(
                row: &::pgkit::__private::sqlx::postgres::PgRow,
            ) -> ::pgkit::__private::sqlx::Result<::core::option::Option<::core::option::Option<Self>>> {
                let present = ::pgkit::try_get_option::<
                    ::core::option::Option<#pk_decode_ty>,
                >(row, ::core::concat!(#table_name, "_", #pk_column))?;
                match present {
                    ::core::option::Option::None => {
                        ::core::result::Result::Ok(::core::option::Option::None)
                    }
                    ::core::option::Option::Some(::core::option::Option::None) => {
                        ::core::result::Result::Ok(::core::option::Option::Some(
                            ::core::option::Option::None,
                        ))
                    }
                    ::core::option::Option::Some(::core::option::Option::Some(_)) => {
                        ::core::result::Result::Ok(::core::option::Option::Some(
                            ::core::option::Option::Some(Self {
                                #(#field_inits_prefixed,)*
                            }),
                        ))
                    }
                }
            }
        }
    }
}
