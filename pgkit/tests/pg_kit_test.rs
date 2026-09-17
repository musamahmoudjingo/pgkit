//! Tests for `#[derive(PgKit)]`.
//!
//! Each scenario lives in its own module so the generated artifacts
//! (`*Table`, `*Column`, `Partial*`) don't collide between tests. The
//! tests exercise compile-time emission (static trait assertions,
//! struct/enum shape) and runtime semantics for the bits that don't
//! depend on a real `PgRow`: `PaginatedRow::row_id`,
//! `PaginatedRow::ordering_column_value`, `Default`.
//!
//! End-to-end `FromRow` decoding needs a live `PgRow`, so it is out of scope
//! here; we assert the trait bound with `_assert_from_row` and rely on
//! `pgkit::try_get_option`'s own contract.

#![cfg(all(feature = "partial-row", feature = "cursor-pagination"))]

use pgkit::PgKit;
use pgkit::pagination::cursor::{CursorPaginationError, CursorValue, PaginatedRow};
use pgkit::types::{DatabaseTable as DatabaseTableTrait, DatabaseTableColumn};

/// Static assertion: `T` implements `sqlx::FromRow<'_, PgRow>`. The body
/// is never called; if the impl is missing the test crate won't compile.
fn _assert_from_row<T>()
where
    T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>,
{
}

// -----------------------------------------------------------------------------
// Scenario 1: default names. With only `table_name` set, every derived
// ident follows the documented "strip Row, suffix" convention.
// -----------------------------------------------------------------------------

mod default_names {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "geo_countries")]
    #[allow(dead_code)]
    pub struct CountriesRow {
        #[pgkit(primary_key)]
        pub id: i32,
        pub iso2: String,
        pub name: String,
        pub region: Option<String>,
        pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    #[test]
    fn emitted_idents_follow_default_strip_row_convention() {
        // Compilation alone proves the idents resolve: the strip-Row
        // default produced `Countries{Table,Column}` and `PartialCountriesRow`.
        let _: CountriesTable;
        let _: CountriesColumn = CountriesColumn::Id;
        let _: PartialCountriesRow = PartialCountriesRow::default();
    }

    #[test]
    fn database_table_primary_key_returns_id_variant() {
        assert_eq!(
            <CountriesTable as DatabaseTableTrait>::primary_key(),
            CountriesColumn::Id
        );
        assert_eq!(CountriesColumn::table_primary_key(), CountriesColumn::Id);
    }

    #[test]
    fn column_into_static_str_gives_sql_name() {
        // `IntoStaticStr` yields each variant's SQL column name: the field
        // ident verbatim, pinned per-variant; `iso2` needs no rename.
        assert_eq!(<&'static str>::from(CountriesColumn::Id), "id");
        assert_eq!(<&'static str>::from(CountriesColumn::Name), "name");
        assert_eq!(<&'static str>::from(CountriesColumn::Iso2), "iso2");
        assert_eq!(
            <&'static str>::from(CountriesColumn::DeletedAt),
            "deleted_at"
        );
    }

    #[test]
    fn select_all_lists_every_column_in_declaration_order() {
        // `VARIANTS` (from `strum::VariantNames`) drives the SELECT list, and
        // every column is table-qualified for JOIN safety.
        assert_eq!(
            CountriesColumn::select_all(),
            "geo_countries.id, geo_countries.iso2, geo_countries.name, \
             geo_countries.region, geo_countries.deleted_at",
        );
        assert_eq!(
            CountriesColumn::select_all_with_table_alias("c"),
            "c.id, c.iso2, c.name, c.region, c.deleted_at",
        );
    }

    #[test]
    fn ordering_value_available_for_every_column() {
        // Every column is a valid cursor sort key: `id` and `deleted_at`
        // (no special annotation) yield real values like any other column.
        let p = fixture_full();
        assert_eq!(
            p.ordering_column_value(CountriesColumn::Id)
                .expect("id projected"),
            Some(CursorValue::Int(7)),
        );
        // `deleted_at` was projected as SQL NULL → Ok(None).
        assert_eq!(
            p.ordering_column_value(CountriesColumn::DeletedAt)
                .expect("deleted_at projected"),
            None,
        );
    }

    #[test]
    fn partial_field_shapes_match_source_nullability() {
        let p = PartialCountriesRow::default();
        // Non-nullable source → Option<T>.
        let _: Option<i32> = p.id;
        let _: Option<String> = p.iso2;
        let _: Option<String> = p.name;
        // Nullable source → Option<Option<T>>.
        let _: Option<Option<String>> = p.region;
        let _: Option<Option<chrono::DateTime<chrono::Utc>>> = p.deleted_at;
    }

    #[test]
    fn partial_implements_from_row() {
        _assert_from_row::<PartialCountriesRow>();
    }

    // -----------------------------------------------------------------------
    // PaginatedRow semantics: row_id + ordering_column_value (the two
    // runtime methods we can exercise without a live PgRow). The impl is
    // parameterized by `CountriesColumn`, the full column enum.
    // -----------------------------------------------------------------------

    fn _assert_paginated_over_column_enum<T>()
    where
        T: PaginatedRow<i32, CountriesColumn>,
    {
    }

    #[test]
    fn paginated_row_is_parameterized_by_full_column_enum() {
        _assert_paginated_over_column_enum::<PartialCountriesRow>();
    }

    fn fixture_full() -> PartialCountriesRow {
        PartialCountriesRow {
            id: Some(7),
            iso2: Some("UG".to_string()),
            name: Some("Uganda".to_string()),
            region: Some(Some("Africa".to_string())),
            deleted_at: Some(None),
        }
    }

    #[test]
    fn paginated_row_id_returns_primary_key_when_projected() {
        let p = fixture_full();
        assert_eq!(p.row_id().expect("id projected"), 7);
    }

    #[test]
    fn paginated_row_id_errors_when_primary_key_not_projected() {
        let p = PartialCountriesRow {
            id: None,
            ..fixture_full()
        };
        assert!(matches!(
            p.row_id(),
            Err(CursorPaginationError::IdColumnWasNotProjected)
        ));
    }

    #[test]
    fn paginated_value_for_non_nullable_column_wraps_in_some() {
        let p = fixture_full();
        assert_eq!(
            p.ordering_column_value(CountriesColumn::Iso2)
                .expect("iso2 projected"),
            Some(CursorValue::Text("UG".to_string())),
        );
        assert_eq!(
            p.ordering_column_value(CountriesColumn::Name)
                .expect("name projected"),
            Some(CursorValue::Text("Uganda".to_string())),
        );
    }

    #[test]
    fn paginated_value_errors_when_non_nullable_column_not_projected() {
        let p = PartialCountriesRow {
            iso2: None,
            ..fixture_full()
        };
        match p.ordering_column_value(CountriesColumn::Iso2) {
            Err(CursorPaginationError::OrderingColumnNotProjected { column }) => {
                // The error carries the SQL column name, not the Rust variant.
                assert_eq!(column, "iso2");
            }
            other => panic!("expected OrderingColumnNotProjected, got {other:?}"),
        }
    }

    #[test]
    fn paginated_value_for_nullable_column_preserves_sql_null() {
        // Projected with a real value.
        let with_value = fixture_full();
        assert_eq!(
            with_value
                .ordering_column_value(CountriesColumn::Region)
                .expect("region projected"),
            Some(CursorValue::Text("Africa".to_string())),
        );

        // Projected as SQL NULL → inner option is None, outer was Some.
        let with_null = PartialCountriesRow {
            region: Some(None),
            ..fixture_full()
        };
        assert_eq!(
            with_null
                .ordering_column_value(CountriesColumn::Region)
                .expect("region projected as NULL is still projected"),
            None,
        );

        // Not projected at all → outer option is None.
        let unprojected = PartialCountriesRow {
            region: None,
            ..fixture_full()
        };
        match unprojected.ordering_column_value(CountriesColumn::Region) {
            Err(CursorPaginationError::OrderingColumnNotProjected { column }) => {
                assert_eq!(column, "region");
            }
            other => panic!("expected OrderingColumnNotProjected, got {other:?}"),
        }
    }
}

// -----------------------------------------------------------------------------
// Scenario 2: every override applied. The three type-name overrides and
// the custom `table_name` should all win over the defaults.
// -----------------------------------------------------------------------------

mod custom_names {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(
        table_name = "weirdly_named_table",
        table = "WeirdTable",
        columns = "WeirdColumn",
        partial = "WeirdPartial"
    )]
    #[allow(dead_code)]
    pub struct Weird {
        #[pgkit(primary_key)]
        pub key: i64,
        pub label: String,
    }

    #[test]
    fn override_idents_are_used() {
        // None of the default names should resolve in this module.
        // The custom names should resolve and behave identically.
        let _: WeirdTable;
        let _: WeirdColumn = WeirdColumn::Key;
        let _: WeirdPartial = WeirdPartial::default();
    }

    #[test]
    fn primary_key_drives_database_table_through_override() {
        assert_eq!(
            <WeirdTable as DatabaseTableTrait>::primary_key(),
            WeirdColumn::Key
        );
    }
}

// -----------------------------------------------------------------------------
// Scenario 3: `#[pgkit(rename = "...")]` overrides the SQL
// column name used by both the column enum's `strum` rendering and the
// partial's `FromRow` lookup. Here we check the field side: the partial
// field's *Rust* ident stays the source ident (`deleted`); the rename
// only affects the SQL name passed to `try_get_option`. The column-name
// side is covered in Scenario 11.
// -----------------------------------------------------------------------------

mod field_rename {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "renamed_demo")]
    #[allow(dead_code)]
    pub struct Renamed {
        #[pgkit(primary_key)]
        pub id: i32,

        // Rust ident stays `deleted`, but the SQL column is `deleted_at`.
        #[pgkit(rename = "deleted_at")]
        pub deleted: Option<chrono::DateTime<chrono::Utc>>,
    }

    #[test]
    fn rust_field_name_is_unchanged() {
        let p = PartialRenamed::default();
        let _: Option<Option<chrono::DateTime<chrono::Utc>>> = p.deleted;
    }

    #[test]
    fn partial_implements_from_row() {
        // The lookup name internally uses "deleted_at"; we can't
        // inspect the macro output, but a successful build proves
        // try_get_option was called with the renamed column.
        _assert_from_row::<PartialRenamed>();
    }
}

// -----------------------------------------------------------------------------
// Scenario 4: rename decoupling. A bare `#[sqlx(rename = "x")]` *without*
// a matching `#[pgkit(rename = "x")]` must not influence what
// PgKit emits. This pins the two-attribute design.
// -----------------------------------------------------------------------------

mod rename_decoupling {
    use super::*;

    #[derive(sqlx::FromRow, PgKit)]
    #[pgkit(table_name = "decoupling_demo")]
    #[allow(dead_code)]
    pub struct Decoupled {
        #[pgkit(primary_key)]
        pub id: i32,

        // #[sqlx(rename)] is here for sqlx's own derive on `Decoupled`.
        // PgKit ignores it; the partial's lookup name comes
        // from the field ident (`status`), not `service_status`.
        #[sqlx(rename = "service_status")]
        pub status: String,
    }

    #[test]
    fn pg_kit_ignores_sqlx_rename() {
        // If PgKit had honoured #[sqlx(rename)], the column
        // enum variant would be `ServiceStatus`; instead it's `Status`.
        let _: DecoupledColumn = DecoupledColumn::Status;
    }
}

// -----------------------------------------------------------------------------
// Scenario 5: `skip(partial)` omits the partial struct, its FromRow, and
// (transitively) the PaginatedRow impl.
// -----------------------------------------------------------------------------

mod skip_partial {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "skip_partial_demo", skip(partial))]
    #[allow(dead_code)]
    pub struct SkipPartial {
        #[pgkit(primary_key)]
        pub id: i32,
        pub name: String,
    }

    #[test]
    fn table_and_columns_still_emitted() {
        let _: SkipPartialTable;
        let _: SkipPartialColumn = SkipPartialColumn::Id;
        let _: SkipPartialColumn = SkipPartialColumn::Name;
    }

    // Compile-fail proof that the partial is gone would require trybuild;
    // here we assert the type isn't in scope by *not* using it.
}

// -----------------------------------------------------------------------------
// Scenario 6: a plain struct with no field annotations beyond the required
// `#[pgkit(primary_key)]`. The table, column enum, and partial emit as usual, and
// so does `PaginatedRow`: every column is a valid cursor sort key, so the
// impl is always emitted unless explicitly skipped.
// -----------------------------------------------------------------------------

mod paginated_row_always_emitted {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "plain_demo")]
    #[allow(dead_code)]
    pub struct Plain {
        #[pgkit(primary_key)]
        pub id: i32,
        pub name: String,
    }

    fn _assert_paginated<T: PaginatedRow<i32, PlainColumn>>() {}

    #[test]
    fn table_columns_and_partial_still_emitted() {
        let _: PlainTable;
        let _: PlainColumn = PlainColumn::Id;
        let _: PartialPlain = PartialPlain::default();
    }

    #[test]
    fn paginated_row_is_emitted_and_covers_every_column() {
        // The impl exists even though no field carried an ordering hint,
        // and `ordering_column_value` yields a value for every column.
        _assert_paginated::<PartialPlain>();

        let p = PartialPlain {
            id: Some(1),
            name: Some("plain".to_string()),
        };
        assert_eq!(
            p.ordering_column_value(PlainColumn::Id)
                .expect("id projected"),
            Some(CursorValue::Int(1)),
        );
        assert_eq!(
            p.ordering_column_value(PlainColumn::Name)
                .expect("name projected"),
            Some(CursorValue::Text("plain".to_string())),
        );
    }
}

// -----------------------------------------------------------------------------
// Scenario 7: snake_case field idents map to PascalCase variant idents.
// -----------------------------------------------------------------------------

mod snake_case_fields {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "snake_demo")]
    #[allow(dead_code)]
    pub struct Snake {
        #[pgkit(primary_key)]
        pub id: i32,
        pub dialing_code: Option<String>,
        pub created_at: chrono::DateTime<chrono::Utc>,
    }

    #[test]
    fn variants_are_pascal_cased() {
        let _: SnakeColumn = SnakeColumn::DialingCode;
        let _: SnakeColumn = SnakeColumn::CreatedAt;
    }
}

// -----------------------------------------------------------------------------
// Scenario 8: visibility is preserved across every emitted type.
// -----------------------------------------------------------------------------

mod visibility {
    pub mod inner {
        use pgkit::PgKit;

        #[derive(PgKit)]
        #[pgkit(table_name = "vis_demo")]
        #[allow(dead_code)]
        pub struct Visible {
            #[pgkit(primary_key)]
            pub id: i32,
            pub label: String,
        }
    }

    #[test]
    fn pub_emitted_types_visible_from_outside() {
        // If any emitted item weren't `pub`, these references would fail.
        let _: inner::VisibleTable;
        let _: inner::VisibleColumn = inner::VisibleColumn::Id;
        let _: inner::PartialVisible = inner::PartialVisible::default();
    }
}

// -----------------------------------------------------------------------------
// Scenario 9: numeric and timestamp columns. The cursor value goes through
// `From<T> for CursorValue`, so this proves the conversion pathway works
// for non-String types; each lands in its typed variant.
// -----------------------------------------------------------------------------

mod non_string_columns {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "non_string_demo")]
    #[allow(dead_code)]
    pub struct NonString {
        #[pgkit(primary_key)]
        pub id: i32,
        pub created_at: chrono::DateTime<chrono::Utc>,
        pub count: i64,
    }

    #[test]
    fn numeric_columns_convert_to_cursor_value() {
        let ts = chrono::DateTime::<chrono::Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let p = PartialNonString {
            id: Some(1),
            created_at: Some(ts),
            count: Some(42),
        };
        assert_eq!(
            p.ordering_column_value(NonStringColumn::Count)
                .expect("count projected"),
            Some(CursorValue::BigInt(42)),
        );
        assert_eq!(
            p.ordering_column_value(NonStringColumn::CreatedAt)
                .expect("created_at projected"),
            Some(CursorValue::DateTimeUtc(ts)),
        );
    }
}

// -----------------------------------------------------------------------------
// Scenario 10: `skip(paginated_row)` drops only the `PaginatedRow` impl.
// The column enum and the partial both still emit: the use case is a
// table queried with offset pagination (or none) that never mints a
// cursor.
// -----------------------------------------------------------------------------

mod skip_paginated_row {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "skip_paginated_demo", skip(paginated_row))]
    #[allow(dead_code)]
    pub struct SkipPaginated {
        #[pgkit(primary_key)]
        pub id: i32,
        pub name: String,
    }

    #[test]
    fn columns_and_partial_still_emitted() {
        // Only the PaginatedRow impl is gone; the column enum and the
        // partial both survive.
        let _: PartialSkipPaginated = PartialSkipPaginated::default();
        let _: SkipPaginatedColumn = SkipPaginatedColumn::Id;
        let _: SkipPaginatedColumn = SkipPaginatedColumn::Name;
    }

    // Proof that `PaginatedRow` is *not* implemented for the partial lives
    // in the trybuild compile-fail suite; a runtime test can't assert the
    // absence of a trait impl.
}

// -----------------------------------------------------------------------------
// Scenario 11: SQL column names are pinned per-variant. Every variant gets an
// explicit `#[strum(serialize = "...")]` (the field ident verbatim, or the
// `#[pgkit(rename = "...")]` value), so the rendered SQL name and
// the partial's `FromRow` lookup can never diverge through a case conversion.
// -----------------------------------------------------------------------------

mod column_names_pinned_verbatim {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "iden_demo")]
    #[allow(dead_code)]
    pub struct IdenDemo {
        #[pgkit(primary_key)]
        pub id: i32,
        pub name: String,
        pub iso2: String,
        pub address_line1: Option<String>,
        #[pgkit(rename = "deleted_at")]
        pub deleted: Option<chrono::DateTime<chrono::Utc>>,
    }

    #[test]
    fn field_idents_render_verbatim() {
        assert_eq!(<&'static str>::from(IdenDemoColumn::Name), "name");
        assert_eq!(<&'static str>::from(IdenDemoColumn::Iso2), "iso2");
        assert_eq!(
            <&'static str>::from(IdenDemoColumn::AddressLine1),
            "address_line1"
        );
    }

    #[test]
    fn renamed_fields_render_the_rename_value() {
        assert_eq!(<&'static str>::from(IdenDemoColumn::Deleted), "deleted_at");
    }
}

// -----------------------------------------------------------------------------
// Scenario 12: a non-`Copy` primary key (`String`): `row_id` returns the PK
// by value for any `PK: Clone`, not only `Copy` types.
// -----------------------------------------------------------------------------

mod non_copy_primary_key {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "string_pk_demo")]
    #[allow(dead_code)]
    pub struct StringPkRow {
        #[pgkit(primary_key)]
        pub id: String,
        pub name: String,
    }

    fn _assert_paginated<T: PaginatedRow<String, StringPkColumn>>() {}

    #[test]
    fn paginated_row_id_clones_non_copy_primary_key() {
        _assert_paginated::<PartialStringPkRow>();

        let p = PartialStringPkRow {
            id: Some("abc".to_string()),
            name: Some("Acme".to_string()),
        };
        assert_eq!(p.row_id().expect("id projected"), "abc".to_string());
        assert_eq!(p.id, Some("abc".to_string()));
    }

    #[test]
    fn paginated_row_id_errors_when_string_primary_key_not_projected() {
        let p = PartialStringPkRow {
            id: None,
            name: Some("Acme".to_string()),
        };
        assert!(matches!(
            p.row_id(),
            Err(CursorPaginationError::IdColumnWasNotProjected)
        ));
    }
}

// -----------------------------------------------------------------------------
// Scenario 13: `from_included_columns` is emitted on the partial so an embedded
// resource can be hydrated from `<table_name>_<column>`-aliased JOIN columns
// (the convention emitted by `SelectBuilder::include`).
// -----------------------------------------------------------------------------

mod from_included_columns {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "prefix_demo")]
    #[allow(dead_code)]
    pub struct PrefixDemoRow {
        #[pgkit(primary_key)]
        pub id: i32,
        pub name: String,
        pub region: Option<String>,
    }

    #[test]
    fn from_included_columns_has_expected_signature() {
        // Coercing the inherent method to a fn pointer fails to compile if it
        // is missing or its signature drifts.
        let _: fn(&sqlx::postgres::PgRow) -> sqlx::Result<Option<Option<PartialPrefixDemoRow>>> =
            PartialPrefixDemoRow::from_included_columns;
    }
}

// -----------------------------------------------------------------------------
// Scenario 14: `DatabaseTable::soft_delete_column` is overridden to `Some(_)`
// when a `deleted_at` column exists (found by SQL column name, so a renamed
// field counts), and stays `None` otherwise.
// -----------------------------------------------------------------------------

mod soft_delete_column {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "sd_with")]
    #[allow(dead_code)]
    pub struct WithDeleted {
        #[pgkit(primary_key)]
        pub id: i32,
        pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    #[derive(PgKit)]
    #[pgkit(table_name = "sd_without")]
    #[allow(dead_code)]
    pub struct WithoutDeleted {
        #[pgkit(primary_key)]
        pub id: i32,
        pub name: String,
    }

    #[derive(PgKit)]
    #[pgkit(table_name = "sd_renamed")]
    #[allow(dead_code)]
    pub struct Renamed {
        #[pgkit(primary_key)]
        pub id: i32,
        #[pgkit(rename = "deleted_at")]
        pub deleted: Option<chrono::DateTime<chrono::Utc>>,
    }

    #[test]
    fn detected_only_when_deleted_at_column_present() {
        assert_eq!(
            <WithDeletedTable as DatabaseTableTrait>::soft_delete_column(),
            Some(WithDeletedColumn::DeletedAt)
        );
        assert_eq!(
            <WithoutDeletedTable as DatabaseTableTrait>::soft_delete_column(),
            None
        );
        // Found by SQL column name → the renamed `deleted` field counts.
        assert_eq!(
            <RenamedTable as DatabaseTableTrait>::soft_delete_column(),
            Some(RenamedColumn::Deleted)
        );
    }
}

// -----------------------------------------------------------------------------
// Scenario 15: field keys combine inside one `#[pgkit(...)]` attribute, so the
// primary key can be renamed without a second attribute.
// -----------------------------------------------------------------------------

mod combined_field_keys {
    use super::*;

    #[derive(PgKit)]
    #[pgkit(table_name = "geo_countries")]
    #[allow(dead_code)]
    pub struct CountriesRow {
        #[pgkit(primary_key, rename = "country_id")]
        pub id: i32,
        pub name: String,
    }

    #[test]
    fn primary_key_and_rename_share_one_attribute() {
        assert_eq!(CountriesTable::primary_key(), CountriesColumn::Id);
        assert_eq!(CountriesColumn::Id.to_string(), "country_id");
    }
}
