use std::future::Future;

use super::backoff::backoff;
use super::policy::RetryPolicy;
use crate::errors::RepositoryResult;

/// Run `op`, retrying per `policy` while the error is retryable
/// ([`RepositoryError::is_retryable`] with the policy's idempotency
/// assertion) and attempts remain. The engine behind
/// [`#[pgkit::retry]`](macro@crate::retry); call it directly when the
/// attribute can't express the shape:
///
/// ```no_run
/// # use std::time::Duration;
/// # use pgkit::errors::RepositoryResult;
/// # use pgkit::retry::{BackoffPolicy, RetryPolicy};
/// # async fn fetch_row(id: i32) -> RepositoryResult<String> { Ok(id.to_string()) }
/// # async fn demo(id: i32) -> RepositoryResult<()> {
/// let row = pgkit::retry::run(
///     RetryPolicy::new(3, BackoffPolicy::Exponential, Duration::from_millis(100), Duration::from_secs(10)),
///     || async { fetch_row(id).await },
/// )
/// .await?;
/// # Ok(()) }
/// ```
///
/// [`RepositoryError::is_retryable`]: crate::errors::RepositoryError::is_retryable
pub async fn run<T, F, Fut>(policy: RetryPolicy, mut op: F) -> RepositoryResult<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = RepositoryResult<T>>,
{
    let mut attempt: u32 = 1;
    loop {
        match op().await {
            Err(err) if attempt < policy.tries() && err.is_retryable(policy.is_idempotent()) => {
                backoff(&policy, attempt, &err).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}
