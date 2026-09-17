//! # Naming defaults
//!
//! When the user doesn't supply explicit overrides, names are derived from the input ident:
//!
//! | Input ident   | base       | table          | columns          | partial               |
//! |---------------|------------|----------------|------------------|-----------------------|
//! | `CountryRow`  | `Country`  | `CountryTable` | `CountryColumn`  | `PartialCountryRow`   |
//! | `Mailbox`     | `Mailbox`  | `MailboxTable` | `MailboxColumn`  | `PartialMailbox`      |
//!
//! These are the **type** names. **Column** names are the field idents
//! verbatim; `#[pgkit(rename = "…")]` overrides one when the SQL
//! column differs from the Rust field. See the `PgKit` derive docs
//! (crate root) for the full attribute reference.

use proc_macro2::Span;
use syn::{
    Data, DeriveInput, Error, Field, Fields, GenericArgument, Ident, PathArguments, Type, TypePath,
    Visibility, spanned::Spanned,
};

use super::attrs::{ContainerAttrs, FieldAttrs, SkipFlags};

pub(crate) struct ResolvedNames {
    pub table: Ident,
    pub columns: Ident,
    pub partial: Ident,
}

/// A single parsed field of the source struct.
pub(crate) struct ModelField {
    pub ident: Ident,
    pub ty: Type,
    pub vis: Visibility,
    /// `Some(T)` when `ty` is `Option<T>`. Drives the nullable branch in
    /// the partial (single vs. double Option) and the order-by cursor
    /// value (return inner option directly vs. wrap in `Some`).
    pub option_inner: Option<Type>,
    /// SQL column name. `#[pgkit(rename = "x")]` if present,
    /// otherwise the field's ident as a string.
    pub column_name: String,
    /// The column-enum variant ident derived from the field ident
    /// (PascalCase). Pre-computed once because it's used by every emitter
    /// that touches the column enum.
    pub variant_ident: Ident,
    /// `#[pgkit(json)]`: carry the cursor value as
    /// `CursorValue::Json` instead of requiring `From<T> for CursorValue`.
    pub is_json: bool,
}

/// Fully-parsed input ready for code-gen.
pub(crate) struct Model {
    /// Visibility of the source struct, propagated to every emitted type.
    pub vis: Visibility,
    /// Resolved names for every artifact we might emit.
    pub names: ResolvedNames,
    /// SQL table name, e.g. `"geo_countries"`. Required.
    pub table_name: String,
    /// Items the user asked us not to emit.
    pub skip: SkipFlags,
    /// Every non-skipped field of the source struct.
    pub fields: Vec<ModelField>,
    /// Index into [`Self::fields`] of the `#[pgkit(primary_key)]` field.
    pub primary_key_index: usize,
}

impl Model {
    /// Parse and validate a [`DeriveInput`].
    pub(crate) fn from_derive_input(input: &DeriveInput) -> Result<Self, Error> {
        if !input.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &input.generics,
                "PgKit cannot be derived on a generic type. The generated \
                 impls (DatabaseTable, DatabaseTableColumn, FromRow, PaginatedRow) \
                 are all non-generic; derive on a concrete row type instead.",
            ));
        }

        let data = match &input.data {
            Data::Struct(d) => d,
            Data::Enum(_) | Data::Union(_) => {
                return Err(Error::new(
                    input.span(),
                    "PgKit can only be derived on structs with named fields",
                ));
            }
        };

        let raw_fields = match &data.fields {
            Fields::Named(named) => &named.named,
            Fields::Unnamed(_) | Fields::Unit => {
                return Err(Error::new(
                    input.span(),
                    "PgKit requires a struct with named fields",
                ));
            }
        };

        let container = ContainerAttrs::parse(&input.attrs)?;

        let mut fields: Vec<ModelField> = Vec::with_capacity(raw_fields.len());
        let mut primary_key_index: Option<usize> = None;
        // Track the span of the first #[pgkit(primary_key)] so the second one can
        // point at *itself* and reference the first by location.
        let mut first_pk_span: Option<Span> = None;

        for (index, field) in raw_fields.iter().enumerate() {
            let attrs = FieldAttrs::parse(&field.attrs)?;
            let model_field = build_field(field, attrs.column_rename.clone(), attrs.is_json)?;

            if attrs.is_primary_key {
                if let Some(_prev) = primary_key_index {
                    // Span on the duplicate attribute, not the field; that
                    // tells the user which one to delete.
                    let dup_span = attrs.primary_key_span.unwrap_or_else(|| field.span());
                    let mut err = Error::new(
                        dup_span,
                        "PgKit requires exactly one #[pgkit(primary_key)] field; \
                         found more than one",
                    );
                    if let Some(first) = first_pk_span {
                        err.combine(Error::new(first, "first #[pgkit(primary_key)] is here"));
                    }
                    return Err(err);
                }
                primary_key_index = Some(index);
                first_pk_span = attrs.primary_key_span;
            }

            fields.push(model_field);
        }

        let primary_key_index = primary_key_index.ok_or_else(|| {
            Error::new(
                input.ident.span(),
                "PgKit requires exactly one field annotated with #[pgkit(primary_key)]",
            )
        })?;

        let names = resolve_names(&input.ident, &container);

        let table_name = container.table_name.ok_or_else(|| {
            Error::new(
                input.ident.span(),
                "PgKit requires `#[pgkit(table_name = \"...\")]` \
                 on the struct; the SQL table name has no sensible default",
            )
        })?;

        Ok(Self {
            vis: input.vis.clone(),
            names,
            table_name,
            skip: container.skip,
            fields,
            primary_key_index,
        })
    }

    /// Convenience accessor for the primary-key field.
    pub(crate) fn primary_key_field(&self) -> &ModelField {
        &self.fields[self.primary_key_index]
    }
}

/// Build a [`ModelField`] from a `syn::Field`, computing the option-inner
/// shape and the column-enum variant ident eagerly.
fn build_field(field: &Field, rename: Option<String>, is_json: bool) -> Result<ModelField, Error> {
    // Field idents are guaranteed by the Fields::Named guard upstream.
    let ident = field.ident.clone().expect("named field");
    let ty = field.ty.clone();
    let option_inner = extract_option_inner(&ty).cloned();
    let column_name = rename.unwrap_or_else(|| ident.to_string());
    let variant_ident = snake_to_pascal_ident(&ident);

    Ok(ModelField {
        ident,
        ty,
        vis: field.vis.clone(),
        option_inner,
        column_name,
        variant_ident,
        is_json,
    })
}

/// Resolve container overrides against defaults, producing the four
/// idents the emitters need.
fn resolve_names(input_ident: &Ident, container: &ContainerAttrs) -> ResolvedNames {
    let base = strip_row_suffix(&input_ident.to_string());

    let table = container
        .table_ident
        .clone()
        .unwrap_or_else(|| Ident::new(&format!("{base}Table"), input_ident.span()));

    let columns = container
        .columns_ident
        .clone()
        .unwrap_or_else(|| Ident::new(&format!("{base}Column"), input_ident.span()));

    // Partial defaults preserve the full input ident: `PartialCountryRow`
    // reads better than `PartialCountry` next to a `CountryRow` source.
    let partial = container
        .partial_ident
        .clone()
        .unwrap_or_else(|| Ident::new(&format!("Partial{}", input_ident), input_ident.span()));

    ResolvedNames {
        table,
        columns,
        partial,
    }
}

/// Strip a trailing `Row` from a struct name (`CountryRow` → `Country`).
/// Used only for default names; every override path bypasses this.
fn strip_row_suffix(name: &str) -> String {
    name.strip_suffix("Row").unwrap_or(name).to_string()
}

/// Convert `snake_case_field` to `SnakeCaseField` for use as an enum
/// variant ident. Underscores are dropped; the first character after each
/// underscore (and the very first character) is uppercased.
fn snake_to_pascal_ident(ident: &Ident) -> Ident {
    let raw = ident.to_string();
    let mut out = String::with_capacity(raw.len());
    let mut capitalize_next = true;
    for ch in raw.chars() {
        if ch == '_' {
            capitalize_next = true;
            continue;
        }
        if capitalize_next {
            out.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }
    Ident::new(&out, ident.span())
}

/// Match `Option<T>` and return `T`. Recognises any path ending in the
/// `Option` segment so `::std::option::Option<T>` and bare `Option<T>`
/// both work. Returns `None` for anything else.
fn extract_option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return None;
    };
    let last = path.segments.last()?;
    if last.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}
