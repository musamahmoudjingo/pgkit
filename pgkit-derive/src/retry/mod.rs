//! `#[pgkit::retry(...)]`: wraps an async repository method in a retry loop.
//!
//! The function body becomes the operation passed to `pgkit::retry::run`,
//! which re-executes it while attempts remain and the returned
//! `RepositoryError::is_retryable(idempotent)` holds. Everything else
//! (signature, generics, other attributes, early `return`s inside the body)
//! is preserved: a `return` inside the body exits that attempt.
//!
//! Two accepted shapes:
//!
//! - A plain `async fn` (free function or inherent method).
//! - A method already desugared by `#[async_trait]`: a non-async fn whose
//!   body builds a `Box::pin(async move { ... })` future. `async_trait`
//!   expands before this macro (it sits on the impl block, which is outer),
//!   so the retry wrapping is applied *inside* that future.

mod args;
mod async_trait;
mod expand;

pub(crate) use expand::expand;
