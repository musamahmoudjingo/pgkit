use std::borrow::Cow;
use std::fmt;

use super::DatabaseTable;

/// A table reference that can be rendered into SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableRef {
    /// Raw, caller-provided string like `"geo_countries"` or `"geo_countries c"`.
    /// Carries no structure: [`name`](Self::name) and [`alias`](Self::alias)
    /// both return the raw string as-is.
    ///
    /// ```
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
    /// use pgkit::types::TableRef;
    ///
    /// let bare:     TableRef = "geo_countries".into();
    /// let bare_al:  TableRef = "geo_countries c".into();
    /// let from_dt:  TableRef = CountriesTable.into();
    ///
    /// assert_eq!(bare.name(), "geo_countries");
    /// assert_eq!(bare_al, TableRef::Bare("geo_countries c".into()));
    /// assert_eq!(from_dt.name(), "geo_countries");
    /// ```
    Bare(Cow<'static, str>),

    /// Created from a [`DatabaseTable`] through one of the `From` impl.
    /// The table name comes from `T::NAME`, so [`name`](Self::name)
    /// and [`alias`](Self::alias) stay correct.
    ///
    /// ```
    /// # #[derive(pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, name: String }
    /// use pgkit::types::TableRef;
    ///
    /// let dt_alias: TableRef = (CountriesTable, "c").into();
    /// assert_eq!(dt_alias.name(), "geo_countries");
    /// assert_eq!(dt_alias.alias(), "c");
    /// ```
    Aliased {
        name: Cow<'static, str>,
        alias: Cow<'static, str>,
    },
}

impl TableRef {
    /// The un-aliased table name.
    pub fn name(&self) -> &str {
        match self {
            Self::Bare(name) => name,
            Self::Aliased { name, .. } => name,
        }
    }

    /// The table's alias if present, otherwise the table name.
    pub fn alias(&self) -> &str {
        match self {
            Self::Bare(name) => name,
            Self::Aliased { alias, .. } => alias,
        }
    }
}

impl fmt::Display for TableRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bare(name) => f.write_str(name),
            Self::Aliased { name, alias } => write!(f, "{name} {alias}"),
        }
    }
}

/// Create a `TableRef` from a `&'static str`.
impl From<&'static str> for TableRef {
    fn from(s: &'static str) -> Self {
        Self::Bare(Cow::Borrowed(s))
    }
}

/// Create a `TableRef` from a `String`.
///
/// **Warning:** the string is spliced into SQL verbatim, so it must never be
/// built from runtime/user input; that is an SQL-injection vector and bypasses
/// the rename-safety of the typed `*Table` structs. Prefer `From<T:
/// DatabaseTable>`; reserve this for static identifiers the builder cannot
/// express.
impl From<String> for TableRef {
    fn from(s: String) -> Self {
        Self::Bare(Cow::Owned(s))
    }
}

/// Create a `TableRef` from a `T: DatabaseTable`.
impl<T: DatabaseTable> From<T> for TableRef {
    fn from(_: T) -> Self {
        Self::Bare(Cow::Borrowed(T::NAME))
    }
}

/// Create a `TableRef` from a `(T: DatabaseTable, alias)` tuple.
impl<T: DatabaseTable> From<(T, &'static str)> for TableRef {
    fn from((_, alias): (T, &'static str)) -> Self {
        Self::Aliased {
            name: Cow::Borrowed(T::NAME),
            alias: Cow::Borrowed(alias),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeTable;

    #[test]
    fn bare_displays_as_name() {
        let t: TableRef = "geo_countries".into();
        assert_eq!(t.to_string(), "geo_countries");
        assert_eq!(t.name(), "geo_countries");
        assert_eq!(t.alias(), "geo_countries");
    }

    #[test]
    fn owned_string_works() {
        let t: TableRef = String::from("geo_countries").into();
        assert_eq!(t.to_string(), "geo_countries");
    }

    #[test]
    fn database_table_becomes_bare() {
        let t: TableRef = FakeTable.into();
        assert_eq!(t.to_string(), "fake_table");
        assert_eq!(t.name(), "fake_table");
        assert_eq!(t.alias(), "fake_table");
    }

    #[test]
    fn database_table_tuple_becomes_aliased() {
        let t: TableRef = (FakeTable, "ft").into();
        assert_eq!(t.to_string(), "fake_table ft");
        assert_eq!(t.name(), "fake_table");
        assert_eq!(t.alias(), "ft");
    }
}
