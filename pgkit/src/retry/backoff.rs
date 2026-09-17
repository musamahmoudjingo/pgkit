use super::policy::RetryPolicy;
use crate::errors::RepositoryError;

/// Log the transient failure and sleep out the backoff delay for `attempt`.
/// Called by the code `#[pgkit::retry]` generates between attempts.
pub async fn backoff(policy: &RetryPolicy, attempt: u32, error: &RepositoryError) {
    let delay = policy.delay(attempt);
    tracing::warn!(
        attempt,
        max_tries = policy.tries(),
        delay_ms = delay.as_millis() as u64,
        error = %error,
        "retrying repository operation after transient error"
    );
    tokio::time::sleep(delay).await;
}
