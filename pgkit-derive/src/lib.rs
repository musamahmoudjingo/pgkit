use proc_macro::TokenStream;

mod db_init;
mod pg_kit;
mod retry;

/// Derives a table type, a column enum, and a partial row from a row struct.
///
/// The `PgKit` derive generates the pieces you need to work with a table:
/// - A table unit struct implementing `DatabaseTable`; it carries the SQL
///   table name as the `NAME` associated constant
/// - A columns enum implementing `DatabaseTableColumn`, deriving `strum`
///   (`IntoStaticStr` + `VariantNames`, plus `Display` + `EnumString`) so
///   the SQL column names live on the enum itself
/// - A partial row + `FromRow` for fetching a partial row data from the DB,
///   implementing `PaginatedRow` (parameterized by the column enum) to be
///   used in cursor pagination
///
/// All of these are generated from a single source-of-truth struct,
/// which also serves as the complete row representation for the table.
///
/// # Container attributes
///
/// On the struct, inside `#[pgkit(...)]`:
///
/// | Key                                            | Purpose                                                                                          |
/// |------------------------------------------------|--------------------------------------------------------------------------------------------------|
/// | `table_name = "..."`                           | **Required.** SQL table name (e.g. `"geo_countries"`).                                           |
/// | `table = "Ident"`                              | Override the table unit struct ident. Default: source ident with `Row` stripped, then `Table`.   |
/// | `columns = "Ident"`                            | Override the column enum ident. Default: source ident with `Row` stripped, then `Column`.        |
/// | `partial = "Ident"`                            | Override the partial struct ident. Default: `Partial` + the full source ident.                   |
/// | `skip(partial, paginated_row)`                 | Don't emit the listed artifacts. `skip(table)` and `skip(columns)` are rejected; both are       |
/// |                                                | structural, derived from the row and `table_name` and named by each other's trait impls.         |
///
/// # Field attributes
///
/// | Attribute                  | Effect                                                                         |
/// |----------------------------|--------------------------------------------------------------------------------|
/// | `#[pgkit(primary_key)]`    | **Required exactly once.** Field type becomes `DatabaseTable::primary_key()`'s |
/// |                            | column variant and `PaginatedRow<Id, _>`'s `Id`.                               |
/// | `#[pgkit(rename = "...")]` | Set the SQL column name explicitly. Drives both the partial's `FromRow` lookup |
/// |                            | and the column enum's rendered name. Needed only when the SQL column differs   |
/// |                            | from the Rust field ident.                                                     |
/// | `#[pgkit(json)]`           | Mark a JSON/JSONB column. Its cursor value is carried as `CursorValue::Json`   |
/// |                            | (the field type need only be `Serialize`), instead of requiring                |
/// |                            | `From<T> for CursorValue`, which a foreign newtype can't supply (orphan rule). |
///
/// Keys can share one attribute: `#[pgkit(primary_key, rename = "country_id")]`.
///
/// The SQL column name is the field ident verbatim (or the `rename` value);
/// every column-enum variant pins it with an explicit
/// `#[strum(serialize = "...")]` + `#[serde(rename = "...")]`, so the rendered
/// SQL name and the partial's `FromRow` lookup can never diverge.
///
/// The above is about *column* names. The derived *type* names
/// (table/columns/partial) follow the defaults tabulated in the
/// `pg_kit::model` module.
///
/// Other attributes (`#[sqlx(...)]`, `#[serde(...)]`, `#[doc = ...]`) are
/// ignored by this derive. If a column needs renaming in `sqlx`'s derive
/// *and* `PgKit`, supply both `#[sqlx(rename = "x")]` and
/// `#[pgkit(rename = "x")]`.
///
/// # Example
///
/// ```
/// use pgkit::PgKit;
/// use pgkit::types::DatabaseTable;
///
/// #[derive(PgKit)]
/// #[pgkit(table_name = "geo_countries")]
/// pub struct CountryRow {
///     #[pgkit(primary_key)]
///     pub id: i32,
///
///     pub iso2: String,
///
///     pub name: String,
///
///     pub region: Option<String>,
///
///     #[pgkit(rename = "deleted_at")]
///     pub deleted: Option<chrono::DateTime<chrono::Utc>>,
/// }
///
/// // Emits:
/// //   pub struct CountryTable;         (impl DatabaseTable)
/// //   pub enum   CountryColumn { Id, Iso2, Name, Region, Deleted } (impl DatabaseTableColumn)
/// //   pub struct PartialCountryRow { id: Option<i32>, …, deleted: Option<Option<DateTime<Utc>>> }
/// //   impl sqlx::FromRow<PgRow>             for PartialCountryRow
/// //   impl PaginatedRow<i32, CountryColumn> for PartialCountryRow
///
/// assert_eq!(CountryTable::NAME, "geo_countries");
/// assert_eq!(CountryColumn::Deleted.to_string(), "deleted_at");
/// assert!(PartialCountryRow::default().id.is_none());
/// ```
#[proc_macro_derive(PgKit, attributes(pgkit))]
pub fn derive_pg_kit(input: TokenStream) -> TokenStream {
    pg_kit::expand(input)
}

/// Wraps an async function returning `RepositoryResult<T>` in a retry loop.
///
/// Expands to `pgkit::retry::run(policy, || async { body })`: the body
/// re-executes while the error is retryable and attempts remain; permanent
/// errors (constraint violations, missing rows, …) return immediately.
/// Between attempts a `tracing` warning is logged and the backoff delay
/// slept.
///
/// # Retry safety
///
/// By default only errors where the statement **definitely did not apply**
/// are retried: pool/connect rejections, deadlocks (`40P01`), and
/// serialization failures (`40001`). Mid-statement failures (I/O, TLS,
/// dropped connections) are ambiguous (the database may have committed the
/// work before the link died), so they are only retried when the method is
/// marked `idempotent`, which asserts re-executing is safe. See
/// `RepositoryError::retry_safety`.
///
/// # Arguments
///
/// | Key            | Default         | Meaning                                              |
/// |----------------|-----------------|------------------------------------------------------|
/// | `tries`        | `3`             | Total attempts, including the first execution.       |
/// | `backoff`      | `"linear"`      | `"fixed"`, `"linear"`, or `"exponential"`.           |
/// | `delay_ms`     | `100`           | Base delay between attempts, in milliseconds.        |
/// | `max_delay_ms` | `10000`         | Upper bound any computed delay is clamped to.        |
/// | `idempotent`   | off             | Also retry ambiguous mid-statement failures.         |
/// | `jitter`       | off             | Randomize each delay by ×[0.5, 1.5).                 |
///
/// # Example
///
/// ```
/// # use pgkit::errors::RepositoryResult;
/// # use tracing::instrument;
/// # struct CountriesRow;
/// # struct CountriesRepository;
/// # impl CountriesRepository {
/// #[instrument(skip(self))]
/// #[pgkit::retry(tries = 3, backoff = "exponential", delay_ms = 50, idempotent, jitter)]
/// async fn view(&self, id: i32) -> RepositoryResult<CountriesRow> {
///     // ...
/// #   Ok(CountriesRow)
/// }
/// # }
/// ```
///
/// # Requirements
///
/// - The function must return `RepositoryResult<T>`; the retry engine
///   classifies the error via `RepositoryError`.
/// - Accepted shapes: a plain `async fn`, or a method inside an
///   `#[async_trait]` impl (the macro detects async_trait's desugaring and
///   wraps the retry loop inside the boxed future).
/// - The body re-executes on retry, so it may only borrow its arguments;
///   moving an owned non-`Copy` argument into the body fails to compile.
///   In `#[async_trait]` impls this extends to owned non-`Copy` parameters
///   themselves (async_trait moves them into the future). Early `return`s
///   inside the body are preserved (they end the attempt).
#[proc_macro_attribute]
pub fn retry(attr: TokenStream, item: TokenStream) -> TokenStream {
    retry::expand(attr, item)
}

/// Embeds a project's migrations as a `Vec<sqlx::migrate::Migration>`.
///
/// Takes the scan root as a string literal, relative to `CARGO_MANIFEST_DIR`,
/// and walks it recursively: every `.sql` file directly inside a folder named
/// `migrations` is a migration, at any depth. The layout around those folders
/// is yours — one flat folder or one per module both work:
///
/// ```text
/// src/
/// ├── billing/
/// │   └── db/migrations/20240101120000_create_invoices.up.sql
/// └── users/
///     └── db/migrations/20240102090000_create_users.up.sql
/// ```
///
/// Each file must be named `<14-digit version>_<name>.up.sql` (the version is
/// the timestamp `sqlx migrate add` generates); versions must be unique across
/// the whole tree. A file starting with `-- no-transaction` runs outside a
/// transaction.
/// The SQL is embedded with `include_str!`, so the binary needs no files at
/// runtime, and each migration is built the way `sqlx::migrate!` builds it,
/// so checksums of applied migrations match.
///
/// A new file only re-expands the macro if the crate's `build.rs` watches the
/// tree: `println!("cargo::rerun-if-changed=src");`.
#[proc_macro]
pub fn migrations(input: TokenStream) -> TokenStream {
    db_init::migrations(input)
}

/// [`migrations!`] wrapped in a ready-to-run `sqlx::migrate::Migrator`.
///
/// ```ignore
/// pgkit::migrator!("src").run(&pool).await?;
/// ```
#[proc_macro]
pub fn migrator(input: TokenStream) -> TokenStream {
    db_init::migrator(input)
}

/// Embeds a project's seeds as a `&'static [pgkit::db_init::seeding::Seed]`.
///
/// Takes the scan root as a string literal, relative to `CARGO_MANIFEST_DIR`,
/// and walks it recursively: every `.sql` file directly inside a folder named
/// `seeds` is a seed, at any depth. Naming is not enforced; files run in
/// path order, so a zero-padded number prefix (`001_roles.sql`,
/// `002_users.sql`) is the recommended way to control the order within a
/// folder. The expression is const, so it can sit in a `static`.
///
/// The same `build.rs` note as [`migrations!`] applies.
#[proc_macro]
pub fn seeds(input: TokenStream) -> TokenStream {
    db_init::seeds(input)
}

/// [`seeds!`] wrapped in a ready-to-run `pgkit::db_init::seeding::Seeder`.
///
/// ```ignore
/// pgkit::seeder!("src").run(&pool).await?;
/// ```
#[proc_macro]
pub fn seeder(input: TokenStream) -> TokenStream {
    db_init::seeder(input)
}
