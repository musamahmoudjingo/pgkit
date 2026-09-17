//! Emits `impl ColumnTypeInfo` for the column enum.
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
//!     pub service_status: ServiceStatus,   // a Postgres enum
//! }
//! ```
//!
//! this emits roughly:
//!
//! ```ignore
//! impl ::pgkit::types::ColumnTypeInfo for CountryColumn {
//!     fn pg_type_info(&self) -> ::pgkit::__private::sqlx::postgres::PgTypeInfo {
//!         match self {
//!             CountryColumn::Id            => <i32           as ::pgkit::__private::sqlx::Type<::pgkit::__private::sqlx::Postgres>>::type_info(),
//!             CountryColumn::ServiceStatus => <ServiceStatus as ::pgkit::__private::sqlx::Type<::pgkit::__private::sqlx::Postgres>>::type_info(),
//!         }
//!     }
//! }
//! ```

use proc_macro2::TokenStream;
use quote::quote;

use super::model::Model;

pub(crate) fn emit(model: &Model) -> TokenStream {
    let columns_ident = &model.names.columns;

    let arms = model.fields.iter().map(|f| {
        let variant = &f.variant_ident;
        // Full field type; `Option<T>: Type` delegates to `T`.
        let ty = &f.ty;
        quote! {
            #columns_ident::#variant =>
                <#ty as ::pgkit::__private::sqlx::Type<::pgkit::__private::sqlx::Postgres>>::type_info()
        }
    });

    quote! {
        #[automatically_derived]
        impl ::pgkit::types::ColumnTypeInfo for #columns_ident {
            fn pg_type_info(&self) -> ::pgkit::__private::sqlx::postgres::PgTypeInfo {
                match self {
                    #(#arms),*
                }
            }
        }
    }
}
