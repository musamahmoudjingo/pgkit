//! Emits `PaginatedRow<Id, Column>` for the partial row.
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
//!     pub name: String,
//!     pub region: Option<String>,
//! }
//! ```
//!
//! this emits roughly:
//!
//! ```ignore
//! impl ::pgkit::pagination::cursor::PaginatedRow<i32, CountryColumn> for PartialCountryRow {
//!     fn row_id(&self) -> Result<i32, CursorPaginationError> {
//!         self.id.clone().ok_or(CursorPaginationError::IdColumnWasNotProjected)
//!     }
//!
//!     fn ordering_column_value(
//!         &self,
//!         column: CountryColumn,
//!     ) -> Result<Option<CursorValue>, CursorPaginationError> {
//!         match column {
//!             CountryColumn::Id => self.id.clone()
//!                 .map(|v| Some(CursorValue::from(v)))
//!                 .ok_or_else(|| CursorPaginationError::OrderingColumnNotProjected {
//!                     column: column.to_string(),
//!                 }),
//!             CountryColumn::Name => self.name.clone()
//!                 .map(|v| Some(CursorValue::from(v)))
//!                 .ok_or_else(|| CursorPaginationError::OrderingColumnNotProjected {
//!                     column: column.to_string(),
//!                 }),
//!             CountryColumn::Region => self.region.clone()
//!                 .map(|inner| inner.map(CursorValue::from))
//!                 .ok_or_else(|| CursorPaginationError::OrderingColumnNotProjected {
//!                     column: column.to_string(),
//!                 }),
//!         }
//!     }
//! }
//! ```
//!
//! `PaginatedRow` is parameterized by the **full column enum**, and pgkit
//! puts no restriction on which column may be a sort key,
//! so `ordering_column_value` accepts any column and the `match` is
//! exhaustive over the enum, with a real value arm for *every* column.
//! There is no catch-all.
//!
//! Because every column can yield a cursor value, **every column's Rust
//! type must be `Into<CursorValue>`**: each arm calls `CursorValue::from`.
//! `CursorValue` has `From` impls for the scalar SQL types out of the box;
//! a column of any other type (e.g. a Postgres `enum` mapped to a custom
//! Rust enum) needs its own `From<T> for CursorValue` impl, or the emitted
//! code won't compile.
//!
//! Two branches in the per-column arm:
//!
//! - **Non-nullable field** (`name: Option<String>` on the partial,
//!   source was `String`): clone, convert to `CursorValue`, and wrap in
//!   `Some` because a missing projection is the only way to get `None`.
//! - **Nullable field** (`region: Option<Option<String>>` on the
//!   partial, source was `Option<String>`): clone, then convert the
//!   inner option's value to `CursorValue` as-is, so SQL `NULL` (the
//!   inner `None`) flows through to the cursor.
//!
//! In both branches the *outer* `Option` answers "was this column
//! projected?": a `None` there becomes `Err(OrderingColumnNotProjected)`.
//!
//! Skipped in two cases:
//!
//! - `skip(paginated_row)`: explicit opt-out. The caller wants the
//!   column enum and partial but never mints cursors (e.g. for offset
//!   pagination or plain `ORDER BY`).
//! - the partial is skipped: `PaginatedRow` is implemented *on* the
//!   partial, so there's no type to implement it for.

use proc_macro2::TokenStream;
use quote::quote;

use super::model::Model;

pub(crate) fn emit(model: &Model) -> TokenStream {
    // `PaginatedRow` is implemented *on* the partial, so a skipped partial
    // leaves nothing to implement it for; `skip(paginated_row)` is the
    // explicit opt-out. Otherwise the impl is always emitted: every
    // column is a valid cursor sort key.
    if model.skip.paginated_row || model.skip.partial {
        return TokenStream::new();
    }

    let partial_ident = &model.names.partial;
    let columns_ident = &model.names.columns;

    let pk_field = model.primary_key_field();
    let pk_ident = &pk_field.ident;
    let pk_ty = &pk_field.ty;

    // One arm per column; the match is exhaustive over the full column enum.
    let value_arms = model.fields.iter().map(|f| {
        let variant = &f.variant_ident;
        let field_ident = &f.ident;

        // A `#[pgkit(json)]` column carries its cursor value as
        // `CursorValue::Json`; the Rust type need only be `Serialize`, not
        // `Into<CursorValue>` (JSON/JSONB types rarely have a `From` impl, and
        // a foreign newtype can't be given one under the orphan rule).
        let to_cursor_value = if f.is_json {
            quote! {
                |v| ::pgkit::pagination::cursor::CursorValue::Json(
                    ::pgkit::__private::serde_json::to_value(&v).unwrap_or(::pgkit::__private::serde_json::Value::Null)
                )
            }
        } else {
            quote! { ::pgkit::pagination::cursor::CursorValue::from }
        };

        // The partial-field shape differs for nullable vs. non-nullable
        // source columns; the cursor value extraction differs to match.
        // `to_cursor_value` is what requires every column type to be
        // convertible: every scalar that can serve as a sort key has a
        // `From<T> for CursorValue` impl; other types must supply one (or be
        // marked `json`).
        if f.option_inner.is_some() {
            // Source is Option<U>; partial field is Option<Option<U>>.
            // Map twice: the outer Option becomes the "projected?" answer
            // we propagate via `ok_or_else`, the inner Option carries the
            // SQL-NULL distinction into the cursor. The error names the
            // offending column via the enum's `Display` impl so callers
            // can produce a self-describing error message.
            quote! {
                #columns_ident::#variant => self
                    .#field_ident
                    .clone()
                    .map(|inner| inner.map(#to_cursor_value))
                    .ok_or_else(|| ::pgkit::pagination::cursor::CursorPaginationError::OrderingColumnNotProjected {
                        column: column.to_string(),
                    }),
            }
        } else {
            // Source is T; partial field is Option<T>. Single map turns
            // `Some(v)` into `Some(Some(v.into()))` so the outer option
            // still signals "projected" and the inner option is always
            // `Some` (a non-nullable column can't be NULL). The error
            // names the offending column via the enum's `Display` impl.
            quote! {
                #columns_ident::#variant => self
                    .#field_ident
                    .clone()
                    .map(|v| ::core::option::Option::Some((#to_cursor_value)(v)))
                    .ok_or_else(|| ::pgkit::pagination::cursor::CursorPaginationError::OrderingColumnNotProjected {
                        column: column.to_string(),
                    }),
            }
        }
    });

    quote! {
        #[automatically_derived]
        impl ::pgkit::pagination::cursor::PaginatedRow<#pk_ty, #columns_ident>
            for #partial_ident
        {
            fn row_id(
                &self,
            ) -> ::core::result::Result<
                #pk_ty,
                ::pgkit::pagination::cursor::CursorPaginationError,
            > {
                self.#pk_ident
                    .clone()
                    .ok_or(::pgkit::pagination::cursor::CursorPaginationError::IdColumnWasNotProjected)
            }

            fn ordering_column_value(
                &self,
                column: #columns_ident,
            ) -> ::core::result::Result<
                ::core::option::Option<::pgkit::pagination::cursor::CursorValue>,
                ::pgkit::pagination::cursor::CursorPaginationError,
            > {
                match column {
                    #(#value_arms)*
                }
            }
        }
    }
}
