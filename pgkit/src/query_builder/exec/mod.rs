//! [`sqlx::QueryBuilder`] wrappers that expose the common terminal
//! methods (`fetch_one`, `fetch_optional`, `fetch_all`, `execute`).
//!
//! Returned by each builder's `build_query` / `build_query_as` /
//! `build_query_scalar` convenience methods:
//!
//! ```no_run
//! # use pgkit::query_builder::Query;
//! # #[derive(sqlx::FromRow, pgkit::PgKit)] #[pgkit(table_name = "geo_countries")] struct CountriesRow { #[pgkit(primary_key)] id: i32, iso2: String, name: String, deleted_at: Option<chrono::DateTime<chrono::Utc>> }
//! # async fn demo(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! let row = Query::insert()
//!     .into(CountriesTable)
//!     .value(CountriesColumn::Iso2, "UG")
//!     .value(CountriesColumn::Name, "Uganda")
//!     .returning_all()
//!     .build_query_as::<CountriesRow>()
//!     .fetch_one(&pool).await?;
//! # Ok(()) }
//! ```
//!
//! To stream call the `.build().build_query_as::<T>().fetch(&pool)`.

mod build_query;
mod build_query_as;
mod build_query_scalar;

pub use build_query::BuildQuery;
pub use build_query_as::BuildQueryAs;
pub use build_query_scalar::BuildQueryScalar;
