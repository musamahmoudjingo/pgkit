use std::borrow::Cow;
use std::fmt;

use super::{DatabaseTable, DatabaseTableColumn};

/// A column reference that can be rendered into SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnRef {
    /// Raw, caller-provided SQL fragment for a column position.
    /// No structure, no validation. Anything the typed variants
    /// can't express goes here: bare names (`"id"`), pre-qualified columns
    /// (`"c.id"`), function calls (`"COUNT(*)"`, `"lower(email)"`), etc.
    ///
    /// ```
    /// use pgkit::types::ColumnRef;
    ///
    /// let bare:      ColumnRef = "status".into();
    /// assert_eq!(bare, ColumnRef::Bare("status".into()));
    ///
    /// let bare_qual: ColumnRef = "c.id".into();
    /// assert_eq!(bare_qual, ColumnRef::Bare("c.id".into()));
    ///
    /// let bare_expr: ColumnRef = "lower(email)".into();
    /// assert_eq!(bare_expr, ColumnRef::Bare("lower(email)".into()));
    /// ```
    Bare(Cow<'static, str>),

    /// A column qualified by its table's name or table alias: `qualifier.column`.
    ///
    /// ```
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
    /// use pgkit::types::ColumnRef;
    ///
    /// let qualified:  ColumnRef = CountriesColumn::Id.into();
    /// assert_eq!(qualified.to_string(), "geo_countries.id");
    ///
    /// let qual_alias: ColumnRef = ("c", CountriesColumn::Id).into();
    /// assert_eq!(qual_alias.to_string(), "c.id");
    /// ```
    Qualified {
        qualifier: Cow<'static, str>,
        column: &'static str,
    },

    /// A qualified column with a projection alias: `qualifier.column AS alias`.
    /// Only renders the `AS alias` part when emitted via [`aliased`](Self::aliased).
    ///
    /// ```
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
    /// use pgkit::types::ColumnRef;
    ///
    /// let by_col:    ColumnRef = (CountriesColumn::Id, "country_id").into();
    /// assert_eq!(by_col.aliased().to_string(), "geo_countries.id AS country_id");
    ///
    /// let by_table:  ColumnRef = (("c", CountriesColumn::Id), "country_id").into();
    /// assert_eq!(by_table.aliased().to_string(), "c.id AS country_id");
    /// ```
    Aliased {
        qualifier: Cow<'static, str>,
        column: &'static str,
        alias: &'static str,
    },
}

impl ColumnRef {
    /// Renders the column reference with an `AS alias` if it has one.
    ///
    /// ```
    /// # use pgkit::types::ColumnRef;
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
    /// let c: ColumnRef = (CountriesColumn::Id, "country_id").into();
    /// assert_eq!(format!("{c}"),             "geo_countries.id");
    /// assert_eq!(format!("{}", c.aliased()), "geo_countries.id AS country_id");
    /// ```
    pub fn aliased(&self) -> impl fmt::Display + '_ {
        DisplayAliased(self)
    }

    /// Renders just the column name (no qualifier, no alias). For write
    /// positions where Postgres rejects `qualifier.column`: INSERT column
    /// lists, UPDATE SET LHS, ON CONFLICT targets.
    ///
    /// ```
    /// # use pgkit::types::ColumnRef;
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
    /// let c: ColumnRef = CountriesColumn::Id.into();
    /// assert_eq!(format!("{c}"),                 "geo_countries.id");
    /// assert_eq!(format!("{}", c.unqualified()), "id");
    /// ```
    pub fn unqualified(&self) -> impl fmt::Display + '_ {
        DisplayUnqualified(self)
    }

    /// Renders `column AS alias` (or just `column` if no alias). For
    /// RETURNING and other projection positions where qualification is
    /// unwanted but `AS alias` support is still needed.
    ///
    /// ```
    /// # use pgkit::types::ColumnRef;
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
    /// let c: ColumnRef = (CountriesColumn::Id, "country_id").into();
    /// assert_eq!(format!("{}", c.unqualified_aliased()), "id AS country_id");
    /// ```
    pub fn unqualified_aliased(&self) -> impl fmt::Display + '_ {
        DisplayUnqualifiedAliased(self)
    }
}

impl fmt::Display for ColumnRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bare(s) => f.write_str(s),
            Self::Qualified { qualifier, column }
            | Self::Aliased {
                qualifier, column, ..
            } => write!(f, "{qualifier}.{column}"),
        }
    }
}

struct DisplayAliased<'a>(&'a ColumnRef);

impl fmt::Display for DisplayAliased<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ColumnRef::Bare(s) => f.write_str(s),
            ColumnRef::Qualified { qualifier, column } => {
                write!(f, "{qualifier}.{column}")
            }
            ColumnRef::Aliased {
                qualifier,
                column,
                alias,
            } => write!(f, "{qualifier}.{column} AS {alias}"),
        }
    }
}

struct DisplayUnqualified<'a>(&'a ColumnRef);

impl fmt::Display for DisplayUnqualified<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ColumnRef::Bare(s) => f.write_str(s),
            ColumnRef::Qualified { column, .. } | ColumnRef::Aliased { column, .. } => {
                f.write_str(column)
            }
        }
    }
}

struct DisplayUnqualifiedAliased<'a>(&'a ColumnRef);

impl fmt::Display for DisplayUnqualifiedAliased<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ColumnRef::Bare(s) => f.write_str(s),
            ColumnRef::Qualified { column, .. } => f.write_str(column),
            ColumnRef::Aliased { column, alias, .. } => write!(f, "{column} AS {alias}"),
        }
    }
}

/// Create a `ColumnRef` from a `&'static str`.
impl From<&'static str> for ColumnRef {
    fn from(s: &'static str) -> Self {
        Self::Bare(Cow::Borrowed(s))
    }
}

/// Create a `ColumnRef` from an owned `String`.
///
/// **Warning:** the string is spliced into SQL verbatim, so it must never be
/// built from runtime/user input; that is an SQL-injection vector and bypasses
/// the rename-safety of the typed `*Column` enums. Prefer `From<C:
/// DatabaseTableColumn>`; reserve this for static identifiers the builder
/// cannot express.
impl From<String> for ColumnRef {
    fn from(s: String) -> Self {
        Self::Bare(Cow::Owned(s))
    }
}

/// Create a qualified `ColumnRef` from a `C: DatabaseTableColumn`, using
/// the column's table name as the qualifier.
///
/// ```
/// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
/// use pgkit::types::ColumnRef;
/// let column: ColumnRef = CountriesColumn::Id.into();
/// assert_eq!(
///     column,
///     ColumnRef::Qualified { qualifier: "geo_countries".into(), column: "id" }
/// );
/// ```
impl<C: DatabaseTableColumn> From<C> for ColumnRef {
    fn from(c: C) -> Self {
        Self::Qualified {
            qualifier: Cow::Borrowed(<C::Table as DatabaseTable>::NAME),
            column: c.into(),
        }
    }
}

/// Create a qualified `ColumnRef` from a `(table_alias, C: DatabaseTableColumn)`
/// tuple, using the provided table alias as the column's qualifier.
///
/// ```
/// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
/// use pgkit::types::ColumnRef;
/// let column: ColumnRef = ("c", CountriesColumn::Id).into();
/// assert_eq!(column, ColumnRef::Qualified { qualifier: "c".into(), column: "id" });
/// ```
impl<C: DatabaseTableColumn> From<(&'static str, C)> for ColumnRef {
    fn from((table_alias, c): (&'static str, C)) -> Self {
        Self::Qualified {
            qualifier: Cow::Borrowed(table_alias),
            column: c.into(),
        }
    }
}

/// Create an aliased `ColumnRef` from a `(C: DatabaseTableColumn, column_alias)`
/// tuple, using the column's table name as the qualifier and the provided
/// `column_alias` as the projection alias. Renders as
/// `qualifier.column AS column_alias` when emitted via [`aliased`](Self::aliased).
///
/// ```
/// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
/// use pgkit::types::ColumnRef;
/// let column: ColumnRef = (CountriesColumn::Id, "country_id").into();
/// assert_eq!(
///     column,
///     ColumnRef::Aliased { qualifier: "geo_countries".into(), column: "id", alias: "country_id" }
/// );
/// ```
impl<C: DatabaseTableColumn> From<(C, &'static str)> for ColumnRef {
    fn from((c, column_alias): (C, &'static str)) -> Self {
        Self::Aliased {
            qualifier: Cow::Borrowed(<C::Table as DatabaseTable>::NAME),
            column: c.into(),
            alias: column_alias,
        }
    }
}

/// Create an aliased `ColumnRef` from a
/// `((table_alias, C: DatabaseTableColumn), column_alias)` tuple. The
/// `table_alias` becomes the column's qualifier; the `column_alias` is the
/// projection alias. Renders as `table_alias.column AS column_alias` when
/// emitted via [`aliased`](Self::aliased).
///
/// ```
/// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
/// use pgkit::types::ColumnRef;
/// let column: ColumnRef = (("c", CountriesColumn::Id), "country_id").into();
/// assert_eq!(
///     column,
///     ColumnRef::Aliased { qualifier: "c".into(), column: "id", alias: "country_id" }
/// );
/// ```
impl<C: DatabaseTableColumn> From<((&'static str, C), &'static str)> for ColumnRef {
    fn from(((table_alias, c), column_alias): ((&'static str, C), &'static str)) -> Self {
        Self::Aliased {
            qualifier: Cow::Borrowed(table_alias),
            column: c.into(),
            alias: column_alias,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeColumn;

    #[test]
    fn bare_displays_as_is() {
        let c: ColumnRef = "status".into();
        assert_eq!(c.to_string(), "status");
        assert_eq!(c.aliased().to_string(), "status");
    }

    #[test]
    fn database_table_column_qualified_by_table_name() {
        let c: ColumnRef = FakeColumn::Id.into();
        assert_eq!(c.to_string(), "fake_table.id");
        assert_eq!(c.aliased().to_string(), "fake_table.id");
    }

    #[test]
    fn alias_tuple_with_column_qualified_by_alias() {
        let c: ColumnRef = ("ft", FakeColumn::Id).into();
        assert_eq!(c.to_string(), "ft.id");
        assert_eq!(c.aliased().to_string(), "ft.id");
    }

    #[test]
    fn column_with_alias_renders_aliased_only_via_aliased() {
        let c: ColumnRef = (FakeColumn::Id, "ft_id").into();
        // Plain Display: safe form, no `AS …`.
        assert_eq!(c.to_string(), "fake_table.id");
        // Aliased rendering: full projection form.
        assert_eq!(c.aliased().to_string(), "fake_table.id AS ft_id");
    }

    #[test]
    fn aliased_qualifier_with_aliased_column() {
        let c: ColumnRef = (("ft", FakeColumn::Id), "ft_id").into();
        assert_eq!(c.to_string(), "ft.id");
        assert_eq!(c.aliased().to_string(), "ft.id AS ft_id");
    }

    #[test]
    fn unqualified_strips_qualifier_for_all_variants() {
        let bare: ColumnRef = "status".into();
        assert_eq!(bare.unqualified().to_string(), "status");

        let qualified: ColumnRef = FakeColumn::Id.into();
        assert_eq!(qualified.unqualified().to_string(), "id");

        let qualified_by_alias: ColumnRef = ("ft", FakeColumn::Id).into();
        assert_eq!(qualified_by_alias.unqualified().to_string(), "id");

        let aliased: ColumnRef = (FakeColumn::Id, "ft_id").into();
        assert_eq!(aliased.unqualified().to_string(), "id");
    }

    #[test]
    fn unqualified_leaves_pre_qualified_bare_unchanged() {
        // Bare is opaque; `unqualified()` doesn't attempt to strip.
        let bare_qualified: ColumnRef = "c.id".into();
        assert_eq!(bare_qualified.unqualified().to_string(), "c.id");
    }

    #[test]
    fn unqualified_aliased_renders_column_and_alias_only() {
        let bare: ColumnRef = "status".into();
        assert_eq!(bare.unqualified_aliased().to_string(), "status");

        let qualified: ColumnRef = FakeColumn::Id.into();
        assert_eq!(qualified.unqualified_aliased().to_string(), "id");

        let aliased: ColumnRef = (FakeColumn::Id, "ft_id").into();
        assert_eq!(aliased.unqualified_aliased().to_string(), "id AS ft_id");

        let aliased_via_alias: ColumnRef = (("ft", FakeColumn::Id), "ft_id").into();
        assert_eq!(
            aliased_via_alias.unqualified_aliased().to_string(),
            "id AS ft_id"
        );
    }
}
