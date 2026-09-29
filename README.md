# pgkit

[![Crates.io](https://img.shields.io/crates/v/pgkit.svg)](https://crates.io/crates/pgkit)
[![Documentation](https://docs.rs/pgkit/badge.svg)](https://docs.rs/pgkit)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Postgres utilities for Rust, built on [sqlx](https://github.com/launchbadge/sqlx).

pgkit is a small toolkit for the repository layer of a Postgres-backed
application: a typed query builder, a derive macro that turns a row struct into
table and column types, offset and cursor pagination, a retry macro for
transient database errors, and an error type that classifies constraint
violations.

It is not an ORM. There are no relations, no change tracking and no migrations.
You write the query, pgkit helps you build it safely and sqlx runs it.

> **Status:** pre-1.0. The API may change between minor versions.

## Features

- **Query builder** for `SELECT`, `INSERT`, `UPDATE` and `DELETE`, with joins,
  CTEs, `UNION`, `ON CONFLICT`, `RETURNING` and row locking. Every value is
  sent as a bound parameter.
- **`#[derive(PgKit)]`** generates a table type, a column enum and a partial
  row from one row struct, so table and column names are checked by the
  compiler instead of being scattered string literals.
- **Pagination**: classic offset pagination, and keyset (cursor) pagination
  with opaque cursors that are validated against the request they come back
  with.
- **`#[pgkit::retry]`** re-runs an async repository method on transient
  failures such as deadlocks and serialization failures.
- **`RepositoryError`** maps Postgres errors to variants like
  `UniqueViolation` and `ForeignKeyViolation`, with the constraint name
  attached.
- **Migrations and seeds**: `migrator!` and `seeder!` find every
  `migrations/` and `seeds/` folder under a path at compile time and embed
  the SQL in the binary, so deploys ship no `.sql` files.
- **Full-text search helper** that turns user input into a safe `to_tsquery`
  string.
- **Connection pool helper** that opens a `PgPool` with production-safe
  defaults: session timeouts against runaway queries, a warm floor of idle
  connections, and connection recycling.

## Installation

```toml
[dependencies]
pgkit = "0.1"
sqlx = { version = "0.9", features = ["postgres", "runtime-tokio", "tls-rustls"] }
```

pgkit depends on sqlx without a runtime or TLS backend, so your application
chooses those through its own sqlx features.

Requires Rust 1.98 or newer.

## Quick start

Describe a table once, with a plain struct:

```rust
use pgkit::PgKit;
use pgkit::query_builder::{Query, WhereOps};

#[derive(PgKit)]
#[pgkit(table_name = "geo_countries")]
pub struct CountriesRow {
    #[pgkit(primary_key)]
    pub id: i32,
    pub iso2: String,
    pub name: String,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

// The derive generated `CountriesTable`, `CountriesColumn` and
// `PartialCountriesRow`. Use them instead of string literals:
let query = Query::select()
    .from(CountriesTable)
    .columns([CountriesColumn::Id, CountriesColumn::Name])
    .where_eq(CountriesColumn::Iso2, "UG")
    .where_null(CountriesColumn::DeletedAt)
    .build();

assert_eq!(
    query.sql(),
    "SELECT geo_countries.id, geo_countries.name FROM geo_countries WHERE geo_countries.iso2 = $1 AND geo_countries.deleted_at IS NULL"
);
```

`build()` returns a regular `sqlx::QueryBuilder`, so anything sqlx can do with
one is still available.

## Running a query

Each builder has `build_query_as`, `build_query_scalar` and `build_query`
shortcuts that go straight to sqlx's fetch methods:

```rust,no_run
use pgkit::PgKit;
use pgkit::query_builder::{Query, WhereOps};

#[derive(sqlx::FromRow, PgKit)]
#[pgkit(table_name = "geo_countries")]
pub struct CountriesRow {
    #[pgkit(primary_key)]
    pub id: i32,
    pub iso2: String,
    pub name: String,
}

async fn find_by_iso2(
    pool: &sqlx::PgPool,
    iso2: &str,
) -> Result<Option<CountriesRow>, sqlx::Error> {
    Query::select()
        .from(CountriesTable)
        .all()
        .where_eq(CountriesColumn::Iso2, iso2)
        .build_query_as::<CountriesRow>()
        .fetch_optional(pool)
        .await
}
```

## Query builder

Plain strings work anywhere a table or column is expected, so you can use the
builder without the derive:

```rust
use pgkit::query_builder::{Query, WhereOps};

// SELECT with a join. Conditions chain in call order.
let select = Query::select()
    .from("addresses a")
    .columns(["a.id", "c.name"])
    .inner_join("geo_countries c", |on| on.eq("a.country_id", "c.id"))
    .where_eq("a.owner_id", 42)
    .where_null("a.deleted_at")
    .build();
assert_eq!(
    select.sql(),
    "SELECT a.id, c.name FROM addresses a INNER JOIN geo_countries c ON a.country_id = c.id WHERE a.owner_id = $1 AND a.deleted_at IS NULL"
);

// INSERT with an upsert.
let insert = Query::insert()
    .into("geo_countries")
    .value("iso2", "UG")
    .value("name", "Uganda")
    .on_conflict(["iso2"])
    .do_update(|u| u.set_excluded("name"))
    .returning_all()
    .build();
assert_eq!(
    insert.sql(),
    "INSERT INTO geo_countries (iso2, name) VALUES ($1, $2) ON CONFLICT (iso2) DO UPDATE SET name = EXCLUDED.name RETURNING *"
);

// UPDATE. `set_if_some` skips the column when the value is `None`,
// which suits PATCH-style endpoints.
let new_name: Option<&str> = None;
let update = Query::update()
    .table("geo_countries")
    .set("iso2", "UG")
    .set_if_some("name", new_name)
    .where_eq("id", 1)
    .build();
assert_eq!(update.sql(), "UPDATE geo_countries SET iso2 = $1 WHERE id = $2");

// DELETE.
let delete = Query::delete().from("geo_countries").where_eq("id", 1).build();
assert_eq!(delete.sql(), "DELETE FROM geo_countries WHERE id = $1");
```

The `WHERE` methods cover comparisons, `IN` / `NOT IN`, `ANY` / `ALL`,
`BETWEEN`, `IS NULL`, `EXISTS`, grouping, conditional clauses and raw
fragments:

```rust
use pgkit::query_builder::{Query, WhereOps};

let include_archived = false;

let query = Query::select()
    .from("users")
    .where_eq("status", "active")
    .where_in("role", vec!["admin", "editor"])
    .or_group(|g| g.where_null("deleted_at").where_gt("age", 18))
    .when(include_archived, |b| b.or_where_not_null("archived_at"))
    .build();

assert_eq!(
    query.sql(),
    "SELECT * FROM users WHERE ((status = $1 AND role IN ($2, $3)) OR (deleted_at IS NULL AND age > $4))"
);
```

### Safety guards

`UPDATE` and `DELETE` panic at build time if their `WHERE` clause ends up
empty, so a condition lost in a refactor cannot silently rewrite or wipe a
table. Call `.all()` to opt into a whole-table statement on purpose.

Column and table names passed as strings are inserted into the SQL as written.
Only pass names you control. Never build them from user input. Values are
always bound as parameters.

## Pagination

Offset pagination:

```rust
use pgkit::pagination::offset::OffsetPagination;
use pgkit::query_builder::{Query, WhereOps};

let query = Query::select()
    .from("geo_countries")
    .all()
    .where_null("deleted_at")
    .paginate(&OffsetPagination::new(2, 20)) // page 2, 20 rows per page
    .build();

assert_eq!(
    query.sql(),
    "SELECT * FROM geo_countries WHERE deleted_at IS NULL LIMIT $1 OFFSET $2"
);
```

Cursor (keyset) pagination stays fast on large tables because it never uses
`OFFSET`. pgkit adds the keyset predicate, a stable `ORDER BY` with the primary
key as tiebreaker, and fetches one extra row to detect whether a next page
exists:

```rust
use pgkit::PgKit;
use pgkit::ordering::{OrderBy, OrderByDirection};
use pgkit::pagination::cursor::CursorPagination;
use pgkit::query_builder::Query;

#[derive(PgKit)]
#[pgkit(table_name = "geo_countries")]
pub struct CountriesRow {
    #[pgkit(primary_key)]
    pub id: i32,
    pub name: String,
}

fn main() -> Result<(), pgkit::pagination::cursor::CursorPaginationError> {
    // `cursor` is the opaque string a client sends back to get the next page.
    let cursor: Option<String> = None;

    let pagination =
        CursorPagination::<CountriesColumn, i32>::with_validated_column_selection::<()>(
            OrderBy::new(CountriesColumn::Name, OrderByDirection::Asc),
            20,
            cursor,
            None,
        )?;

    let query = Query::select()
        .from(CountriesTable)
        .all_columns_of::<CountriesColumn>()
        .cursor_paginate(&pagination)
        .build();

    assert_eq!(
        query.sql(),
        "SELECT geo_countries.id, geo_countries.name FROM geo_countries ORDER BY geo_countries.name ASC NULLS LAST, geo_countries.id ASC NULLS LAST LIMIT $1"
    );
    Ok(())
}
```

After fetching, `pagination.into_response_parts(rows)` trims the extra row and
returns the cursor for the next page, which you `encode()` and hand to the
client. A cursor records the sort column, direction and active filters, so one
that no longer matches the request is rejected instead of returning wrong rows.

## Retrying transient failures

```rust
use pgkit::errors::RepositoryResult;

struct CountriesRepository;

impl CountriesRepository {
    #[pgkit::retry(tries = 3, backoff = "exponential", delay_ms = 50, idempotent, jitter)]
    async fn count(&self) -> RepositoryResult<i64> {
        // ... run a query
        Ok(0)
    }
}
```

By default only errors where the statement definitely did not run are retried:
pool and connection rejections, deadlocks (`40P01`) and serialization failures
(`40001`). Failures that leave the outcome unknown, such as a dropped
connection mid-statement, are retried only when you mark the method
`idempotent`. Constraint violations and missing rows are never retried.

The function must be `async` and return `RepositoryResult<T>`. It also works
inside `#[async_trait]` impls. Backoff sleeps use `tokio::time::sleep`, so the
`retry` feature requires a Tokio runtime.

## Embedded migrations and seeds

The `db-init` feature (off by default) discovers, validates and embeds a
project's SQL at compile time. The convention is, any folder named `migrations`
holds migrations, any folder named `seeds` holds seeds, at any depth. How you
arrange the folders around them is up to you. One flat pair at the root, or
one pair per module:

```text
src/
├── billing/
│   └── db/
│       ├── migrations/
│       │   └── 20240101120000_create_invoices.up.sql
│       └── seeds/
│           └── 001_default_plans.sql
└── users/
    └── db/
        └── migrations/
            └── 20240102090000_create_users.up.sql
```

```rust,ignore
let pool = sqlx::PgPool::connect(&database_url).await?;

pgkit::migrator!("src").run(&pool).await?; // an sqlx::migrate::Migrator
pgkit::seeder!("src").run(&pool).await?;   // a pgkit::db_init::seeding::Seeder
```

Each macro takes one path, relative to the crate's `Cargo.toml`, and walks it
recursively. The SQL is embedded with `include_str!`, so the compiled binary
needs no `.sql` files at runtime, and each migration is built the way
`sqlx::migrate!` builds it, so the checksums recorded in `_sqlx_migrations`
match.

The file rules, enforced at compile time so a bad file fails the build
instead of a deploy:

- A migration is `<version>_<name>.up.sql`, where the version is the
  14-digit timestamp `sqlx migrate add` generates. Versions must be unique
  across the whole tree; the embedded set is sorted by version.
- A migration starting with the line `-- no-transaction` runs outside a
  transaction (for statements such as `CREATE INDEX CONCURRENTLY`).

Seed names are not enforced. Seeds run in path order, so the recommended
naming is a zero-padded number prefix (`001_roles.sql`, `002_users.sql`) to
make the order within a folder explicit.

Seeds have no `_sqlx_migrations`-style tracking: `Seeder::run` executes every
script on every run, so write each script to be a no-op once its data exists:

```sql
INSERT INTO plans (code, name)
SELECT 'standard', 'Standard'
WHERE NOT EXISTS (SELECT 1 FROM plans);
```

One failing script does not stop the rest: `Seeder::run` logs each failure
through `tracing`, runs the remaining scripts, and returns every failure
together as one `SeedErrors` list:

```rust,ignore
if let Err(errors) = pgkit::seeder!("src").run(&pool).await {
    for error in &errors.0 {
        // "error executing seed billing/db/seeds/001_defaults.sql: ..."
        eprintln!("{error}");
    }
}
```

The runtime types live in `pgkit::db_init::seeding`: `Seeder`, `Seed`
(the embedded script — its `path` relative to the scan root, and its `sql`),
`SeedError` and `SeedErrors`.

When you want to compose things yourself, `migrations!` gives you the raw
`Vec<sqlx::migrate::Migration>` and `seeds!` the `&'static [Seed]` that
the two wrapper macros are built from.

One caveat comes with any file-embedding macro, and sqlx documents the same
for `migrate!`: editing an embedded file recompiles automatically, but
_adding or removing_ a file does not — the cached expansion is reused. Give
the crate a two-line `build.rs` so the tree is watched:

```rust,ignore
fn main() {
    println!("cargo::rerun-if-changed=src");
}
```

## Errors

`RepositoryError` converts from `sqlx::Error` and sorts Postgres failures into
variants you can match on:

```rust
use pgkit::errors::{RepositoryError, RepositoryResult};

struct Vendor;

enum AppError {
    DuplicateName,
    Repository(RepositoryError),
}

fn map_create_result(result: RepositoryResult<Vendor>) -> Result<Vendor, AppError> {
    match result {
        Ok(vendor) => Ok(vendor),
        Err(RepositoryError::UniqueViolation { constraint: Some(c), .. })
            if c == "uq_vendors_name" =>
        {
            Err(AppError::DuplicateName)
        }
        Err(e) => Err(AppError::Repository(e)),
    }
}
```

## Full-text search

`build_tsquery` turns free text into a prefix-matching `to_tsquery` string.
Query operators are stripped from each term, so user input cannot inject
syntax, and at most `MAX_TERMS` terms are used:

```rust
use pgkit::full_text_search::build_tsquery;

assert_eq!(build_tsquery("red shoe").as_deref(), Some("red:* & shoe:*"));
assert_eq!(build_tsquery("   "), None);
```

## Connection pool

`pool::get` opens a `PgPool` from your `PgConnectOptions`, and a `PoolConfig`.

```rust,no_run
use pgkit::pool::{self, PoolConfig};
use sqlx::postgres::PgConnectOptions;

async fn connect() -> Result<sqlx::PgPool, sqlx::Error> {
    let options = PgConnectOptions::new()
        .host("localhost")
        .port(5432)
        .username("app")
        .password("password")
        .database("app");

    pool::get(options, PoolConfig {
        max_connections: 20,
        ..PoolConfig::default()
    })
    .await
}
```

The defaults are meant for a server answering requests: a 30-second
`statement_timeout` and a 60-second `idle_in_transaction_session_timeout` are
set on every session, so a runaway query or an abandoned transaction cannot
hold a pooled connection and exhaust the pool. Connections are recycled after
30 minutes, idle ones closed after 10, and a small warm floor (a quarter of
`max_connections`, at most 5) is kept open.

The session timeouts follow Postgres's own convention: `Duration::ZERO`
disables one. A migration runner wants both off, so a long migration is not
killed mid-flight:

```rust,no_run
use std::time::Duration;
use pgkit::pool::{self, PoolConfig};
use sqlx::postgres::PgConnectOptions;

async fn migration_pool() -> Result<sqlx::PgPool, sqlx::Error> {
    let options = PgConnectOptions::new()
        .host("localhost")
        .port(5432)
        .username("app")
        .password("password")
        .database("app");

    pool::get(options, PoolConfig {
        max_connections: 2,
        statement_timeout: Duration::ZERO,
        idle_tx_timeout: Duration::ZERO,
        ..PoolConfig::default()
    })
    .await
}
```

## Feature flags

All of these are on by default, except `db-init`.

| Feature             | Provides                                                           |
| ------------------- | ------------------------------------------------------------------ |
| `partial-row`       | `try_get_option`, and with `cursor-pagination`, `#[derive(PgKit)]` |
| `offset-pagination` | `pagination::offset`                                               |
| `cursor-pagination` | `pagination::cursor`                                               |
| `pagination`        | Both pagination features                                           |
| `repository-error`  | `errors::RepositoryError`                                          |
| `retry`             | `#[pgkit::retry]` and `retry::run` (needs Tokio)                   |
| `serde`             | `Serialize` / `Deserialize` for `OrderByDirection` and similar     |
| `db-init`           | `migrator!` / `seeder!` and `db_init` — off by default             |

The query builder, `ordering`, `projection`, `pool` and `full_text_search`
are always available. To use only the query builder:

```toml
pgkit = { version = "0.1", default-features = false }
```

## Crates in this repository

| Crate                | Purpose                                           |
| -------------------- | ------------------------------------------------- |
| `pgkit`              | The library. This is the one you depend on.       |
| `pgkit-derive`       | The procedural macros, re-exported by `pgkit`.    |
| `pgkit-derive-tests` | Compile-fail tests for the macros. Not published. |

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
