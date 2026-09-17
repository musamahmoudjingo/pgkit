//! Testing utilities.

use crate::types::{DatabaseTable, DatabaseTableColumn};

pub(crate) struct FakeTable;

#[derive(Debug, Clone, Copy, strum::VariantNames, strum::EnumIter)]
pub(crate) enum FakeColumn {
    Id,
    Name,
}

impl From<FakeColumn> for &'static str {
    fn from(c: FakeColumn) -> Self {
        match c {
            FakeColumn::Id => "id",
            FakeColumn::Name => "name",
        }
    }
}

impl DatabaseTableColumn for FakeColumn {
    type Table = FakeTable;
    fn table_primary_key() -> Self {
        FakeColumn::Id
    }
}

impl DatabaseTable for FakeTable {
    type Columns = FakeColumn;
    const NAME: &'static str = "fake_table";
    fn primary_key() -> Self::Columns {
        FakeColumn::Id
    }
}

pub(crate) struct FakeRelatedTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantNames, strum::EnumIter)]
pub(crate) enum FakeRelatedColumn {
    Id,
    Label,
}

impl From<FakeRelatedColumn> for &'static str {
    fn from(c: FakeRelatedColumn) -> Self {
        match c {
            FakeRelatedColumn::Id => "id",
            FakeRelatedColumn::Label => "label",
        }
    }
}

impl DatabaseTableColumn for FakeRelatedColumn {
    type Table = FakeRelatedTable;
    fn table_primary_key() -> Self {
        FakeRelatedColumn::Id
    }
}

impl DatabaseTable for FakeRelatedTable {
    type Columns = FakeRelatedColumn;
    const NAME: &'static str = "fake_related";
    fn primary_key() -> Self::Columns {
        FakeRelatedColumn::Id
    }
}

pub(crate) struct FakeSoftTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantNames, strum::EnumIter)]
pub(crate) enum FakeSoftColumn {
    Id,
    Label,
    DeletedAt,
}

impl From<FakeSoftColumn> for &'static str {
    fn from(c: FakeSoftColumn) -> Self {
        match c {
            FakeSoftColumn::Id => "id",
            FakeSoftColumn::Label => "label",
            FakeSoftColumn::DeletedAt => "deleted_at",
        }
    }
}

impl DatabaseTableColumn for FakeSoftColumn {
    type Table = FakeSoftTable;
    fn table_primary_key() -> Self {
        FakeSoftColumn::Id
    }
}

impl DatabaseTable for FakeSoftTable {
    type Columns = FakeSoftColumn;
    const NAME: &'static str = "fake_soft";
    fn primary_key() -> Self::Columns {
        FakeSoftColumn::Id
    }
    fn soft_delete_column() -> Option<Self::Columns> {
        Some(FakeSoftColumn::DeletedAt)
    }
}

#[cfg(feature = "cursor-pagination")]
mod cursor {
    use serde::{Deserialize, Serialize};

    use crate::types::{ColumnTypeInfo, DatabaseTable, DatabaseTableColumn};

    #[derive(Debug, Copy, Clone)]
    pub(crate) struct CountriesTable;

    impl DatabaseTable for CountriesTable {
        type Columns = CountriesColumn;
        const NAME: &'static str = "geo_countries";
        fn primary_key() -> Self::Columns {
            CountriesColumn::Id
        }
    }

    #[derive(
        Debug,
        Default,
        Copy,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Serialize,
        Deserialize,
        strum::IntoStaticStr,
        strum::VariantNames,
        strum::EnumIter,
    )]
    #[strum(serialize_all = "snake_case")]
    pub(crate) enum CountriesColumn {
        #[default]
        Id,
        Name,
        CreatedAt,
    }

    impl DatabaseTableColumn for CountriesColumn {
        type Table = CountriesTable;
        fn table_primary_key() -> Self {
            CountriesColumn::Id
        }
    }

    impl ColumnTypeInfo for CountriesColumn {
        fn pg_type_info(&self) -> sqlx::postgres::PgTypeInfo {
            // Never invoked: these fixtures sort by scalar values, not the enum CAST path.
            match self {
                CountriesColumn::Id => <i32 as sqlx::Type<sqlx::Postgres>>::type_info(),
                CountriesColumn::Name | CountriesColumn::CreatedAt => {
                    <String as sqlx::Type<sqlx::Postgres>>::type_info()
                }
            }
        }
    }
}

#[cfg(feature = "cursor-pagination")]
pub(crate) use cursor::{CountriesColumn, CountriesTable};
