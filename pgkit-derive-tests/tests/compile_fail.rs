//! Compile-fail diagnostics for `#[derive(PgKit)]`.
//!
//! Each case is a single-file fixture under `compile_tests/`
//! paired with a `.stderr` snapshot. trybuild runs `cargo build` on the
//! fixture and asserts the actual stderr matches.
//!
//! # Updating snapshots
//!
//! If a diagnostic message changes intentionally, regenerate the
//! snapshots with:
//!
//! ```bash
//! TRYBUILD=overwrite cargo test -p pgkit-derive-tests --test compile_fail
//! ```
//!
//! Then commit the updated `.stderr` files. The snapshots are part of the
//! API surface; they're what users actually read when they make a
//! mistake.
//!
//! # Why each case exists
//!
//! - **missing_primary_key**: the most common mistake on a new struct.
//! - **multiple_primary_keys**: guards against silently picking one and
//!   dropping the other.
//! - **missing_table_name**: there's no sane default for the SQL table
//!   name; we want to fail loudly.
//! - **skip_columns**: column enum is structurally required.
//! - **skip_table**: table struct is derived from `table_name` and named
//!   by the column enum's impl; it is structurally required.
//! - **skip_paginated_row_omits_impl**: confirms `skip(paginated_row)`
//!   genuinely omits the `PaginatedRow` impl (a runtime test can't assert
//!   the absence of a trait impl).
//! - **non_convertible_column_type**: every column feeds the emitted
//!   `PaginatedRow::ordering_column_value`, so a column type that isn't
//!   `Into<CursorValue>` must fail to compile.
//! - **generic_struct**: generics on the source would force generic
//!   impls everywhere; we don't support that.
//! - **tuple_struct**: only named-field structs map cleanly to columns.
//! - **non_struct**: enums and unions aren't rows.
//! - **embed_alias_too_long**: a `{table}_{column}` embed alias over
//!   PostgreSQL's 63-byte identifier limit would be silently truncated at
//!   runtime, making `from_included_columns` decode nothing.
//! - **retry_non_async**: `#[pgkit::retry]` awaits between attempts, so a
//!   non-async fn must be rejected up front.
//! - **retry_invalid_backoff**: backoff is a closed set; typos must fail
//!   loudly, not fall back to a default.
//! - **retry_zero_tries**: `tries = 0` would mean "never run the body".

#[test]
fn compile_fail_cases() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_tests/missing_primary_key.rs");
    t.compile_fail("tests/compile_tests/multiple_primary_keys.rs");
    t.compile_fail("tests/compile_tests/missing_table_name.rs");
    t.compile_fail("tests/compile_tests/skip_columns.rs");
    t.compile_fail("tests/compile_tests/skip_table.rs");
    t.compile_fail("tests/compile_tests/skip_paginated_row_omits_impl.rs");
    t.compile_fail("tests/compile_tests/non_convertible_column_type.rs");
    t.compile_fail("tests/compile_tests/generic_struct.rs");
    t.compile_fail("tests/compile_tests/tuple_struct.rs");
    t.compile_fail("tests/compile_tests/non_struct.rs");
    t.compile_fail("tests/compile_tests/embed_alias_too_long.rs");
    t.compile_fail("tests/compile_tests/retry_non_async.rs");
    t.compile_fail("tests/compile_tests/retry_invalid_backoff.rs");
    t.compile_fail("tests/compile_tests/retry_zero_tries.rs");
}
