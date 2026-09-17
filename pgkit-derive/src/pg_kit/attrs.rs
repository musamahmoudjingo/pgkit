//! Attribute parsing for `#[derive(PgKit)]`.
//!
//! Three attributes are recognized:
//!
//! - `#[pgkit(...)]` on the struct: container configuration.
//! - `#[pgkit(rename = "...")]` on a field: column rename.
//! - `#[pgkit(primary_key)]` on exactly one field.
//!
//! Everything else (including `#[sqlx(...)]`) is ignored.

use proc_macro2::Span;
use syn::{Attribute, Error, Ident, LitStr, parse_str};

/// Items the derive can be told not to emit. Only the partial row and its
/// `PaginatedRow` impl are optional; the table struct and column enum are
/// structural (derived from the row and `table_name`) and always emitted.
#[derive(Default)]
pub(crate) struct SkipFlags {
    pub partial: bool,
    /// Skip impl `PaginatedRow` on the partial.
    pub paginated_row: bool,
}

/// Parsed `#[pgkit(...)]` from the struct level.
#[derive(Default)]
pub(crate) struct ContainerAttrs {
    pub table_name: Option<String>,
    pub table_ident: Option<Ident>,
    pub columns_ident: Option<Ident>,
    pub partial_ident: Option<Ident>,
    pub skip: SkipFlags,
}

impl ContainerAttrs {
    /// Walk every `#[pgkit(...)]` on the struct and merge the
    /// recognized keys into a single [`ContainerAttrs`].
    pub(crate) fn parse(attrs: &[Attribute]) -> Result<Self, Error> {
        let mut out = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("pgkit") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                // `table_name = "geo_countries"`: the SQL table name.
                if meta.path.is_ident("table_name") {
                    let lit: LitStr = meta.value()?.parse()?;
                    out.table_name = Some(lit.value());
                    return Ok(());
                }

                // Type-name overrides. Stored as `Ident`s rather than
                // paths because emitters only ever need the bare ident.
                if let Some(slot) = match_ident_override(&meta.path) {
                    let lit: LitStr = meta.value()?.parse()?;
                    let ident: Ident = parse_str(&lit.value()).map_err(|e| {
                        Error::new(lit.span(), format!("expected a valid identifier: {e}"))
                    })?;
                    match slot {
                        IdentSlot::Table => out.table_ident = Some(ident),
                        IdentSlot::Columns => out.columns_ident = Some(ident),
                        IdentSlot::Partial => out.partial_ident = Some(ident),
                    }
                    return Ok(());
                }

                // `skip(partial, paginated_row)`: multi-arg form, parsed
                // with parse_nested_meta so each item gets its own span.
                // `table` and `columns` are structural: they're rejected
                // here, pointing the diagnostic right at the offending item.
                if meta.path.is_ident("skip") {
                    meta.parse_nested_meta(|inner| {
                        if inner.path.is_ident("partial") {
                            out.skip.partial = true;
                        } else if inner.path.is_ident("paginated_row") {
                            out.skip.paginated_row = true;
                        } else if inner.path.is_ident("table") {
                            return Err(inner.error(
                                "skip(table) is not allowed: the table struct is \
                                 derived entirely from `table_name` and is named \
                                 by the column enum's DatabaseTableColumn impl; \
                                 it is always emitted",
                            ));
                        } else if inner.path.is_ident("columns") {
                            return Err(inner.error(
                                "skip(columns) is not allowed: the table and \
                                 paginated_row emitters depend on the column enum",
                            ));
                        } else {
                            return Err(inner.error(
                                "unknown skip target; expected one of: \
                                 partial, paginated_row",
                            ));
                        }
                        Ok(())
                    })?;
                    return Ok(());
                }

                Err(meta.error(
                    "unknown #[pgkit] argument; expected one of: \
                     table_name, table, columns, partial, skip",
                ))
            })?;
        }

        Ok(out)
    }
}

/// Parsed per-field attributes.
#[derive(Default)]
pub(crate) struct FieldAttrs {
    pub is_primary_key: bool,
    /// Span of the `#[pgkit(primary_key)]` attribute itself, used for the
    /// duplicate-primary-key diagnostic.
    pub primary_key_span: Option<Span>,
    /// Value of `#[pgkit(rename = "...")]` on the field.
    pub column_rename: Option<String>,
    /// `#[pgkit(json)]`: a JSON/JSONB column whose Rust type has no
    /// `From<T> for CursorValue`. Its cursor value is carried as
    /// `CursorValue::Json` (the type need only be `Serialize`).
    pub is_json: bool,
}

impl FieldAttrs {
    /// Walk every attribute on a field and pull out the bits we own.
    pub(crate) fn parse(attrs: &[Attribute]) -> Result<Self, Error> {
        let mut out = Self::default();

        for attr in attrs {
            if attr.path().is_ident("pgkit") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("primary_key") {
                        out.is_primary_key = true;
                        out.primary_key_span = Some(meta.path.span());
                        return Ok(());
                    }
                    if meta.path.is_ident("rename") {
                        let lit: LitStr = meta.value()?.parse()?;
                        out.column_rename = Some(lit.value());
                        return Ok(());
                    }
                    if meta.path.is_ident("json") {
                        out.is_json = true;
                        return Ok(());
                    }
                    Err(meta.error("unknown field-level #[pgkit] argument; expected: primary_key, rename, json"))
                })?;
                continue;
            }

            // Other attributes (#[sqlx(...)], #[serde(...)], #[doc = ...])
            // are deliberately ignored; PgKit owns only the
            // attributes listed above.
        }

        Ok(out)
    }
}

/// Which slot in [`ContainerAttrs`] a recognised type-name override
/// belongs to. Pulled into an enum so the parser stays linear.
enum IdentSlot {
    Table,
    Columns,
    Partial,
}

/// Match a path against the three type-override keys and return the
/// corresponding slot, or `None` if the path isn't one of them.
fn match_ident_override(path: &syn::Path) -> Option<IdentSlot> {
    if path.is_ident("table") {
        Some(IdentSlot::Table)
    } else if path.is_ident("columns") {
        Some(IdentSlot::Columns)
    } else if path.is_ident("partial") {
        Some(IdentSlot::Partial)
    } else {
        None
    }
}

// `Spanned` is needed for `attr.path().span()` on field attributes.
use syn::spanned::Spanned;
