//! `#[derive(PgKit)]`: Emits the pieces you'll need to build
//! database queries.
//!
//! Every database-backed module needs the same supporting types around
//! its row struct:
//!
//! - A unit struct for the table (`CountriesTable`) carrying the SQL
//!   table name as the `DatabaseTable::NAME` associated constant.
//! - A column enum (`CountriesColumn`) so callers can name columns
//!   type-safely; its `strum` derives carry the SQL column names.
//! - A partial row (`PartialCountriesRow`) for dynamic-projection queries.
//! - A `PaginatedRow` impl on the partial (parameterized by the column
//!   enum), so cursor pagination can mint next-page cursors.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::{DeriveInput, Error, parse_macro_input};

pub(crate) mod attrs;
pub(crate) mod column_type;
pub(crate) mod columns;
pub(crate) mod model;
pub(crate) mod paginated;
pub(crate) mod partial;
pub(crate) mod table;

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match try_expand(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn try_expand(input: &DeriveInput) -> Result<TokenStream2, Error> {
    let model = model::Model::from_derive_input(input)?;

    let columns = columns::emit(&model);
    let table = table::emit(&model);
    let partial = partial::emit(&model);
    let paginated = paginated::emit(&model);
    let column_type = column_type::emit(&model);

    Ok(quote::quote! {
        #columns
        #table
        #partial
        #paginated
        #column_type
    })
}
