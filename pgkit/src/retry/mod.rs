//! Runtime support for the [`#[pgkit::retry]`](macro@crate::retry) attribute macro.
//!
//! The macro rewrites an async function returning
//! [`RepositoryResult`](crate::errors::RepositoryResult) into a loop that
//! re-executes the body while [`RepositoryError::is_retryable`] holds and
//! attempts remain, sleeping per the configured [`BackoffPolicy`] between attempts.
//! The items here are public so the generated code can name them; they can
//! also be used directly for hand-rolled retry loops.
//!
//! [`RepositoryError::is_retryable`]: crate::errors::RepositoryError::is_retryable

mod backoff;
mod backoff_policy;
mod policy;
mod run;

pub use backoff::backoff;
pub use backoff_policy::BackoffPolicy;
pub use policy::RetryPolicy;
pub use run::run;
