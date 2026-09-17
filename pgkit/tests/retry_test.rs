//! Behavior tests for `#[pgkit::retry]`.
//!
//! Backoff *math* is unit-tested in `pgkit::retry`; these tests pin the
//! macro's loop semantics (what retries, what doesn't, and how many times)
//! using 1ms fixed delays to stay fast.

#![cfg(feature = "retry")]

use std::sync::atomic::{AtomicU32, Ordering};

use pgkit::errors::{RepositoryError, RepositoryResult};

fn transient_error() -> RepositoryError {
    RepositoryError::from(sqlx::Error::PoolTimedOut)
}

/// Retryable only when the operation is asserted idempotent.
fn ambiguous_error() -> RepositoryError {
    RepositoryError::from(sqlx::Error::Io(std::io::Error::other("connection reset")))
}

fn permanent_error() -> RepositoryError {
    RepositoryError::RowNotFound {
        message: "nope".to_string(),
    }
}

/// Fails with the given error until `fail_times` calls have happened, then
/// succeeds returning the total call count.
struct FlakyRepo {
    calls: AtomicU32,
    fail_times: u32,
}

impl FlakyRepo {
    fn new(fail_times: u32) -> Self {
        Self {
            calls: AtomicU32::new(0),
            fail_times,
        }
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }

    #[pgkit::retry(tries = 3, backoff = "fixed", delay_ms = 1)]
    async fn transient_failures(&self) -> RepositoryResult<u32> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call <= self.fail_times {
            return Err(transient_error());
        }
        Ok(call)
    }

    #[pgkit::retry(tries = 3, backoff = "fixed", delay_ms = 1)]
    async fn permanent_failure(&self) -> RepositoryResult<u32> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(permanent_error())
    }

    #[pgkit::retry(tries = 1, backoff = "fixed", delay_ms = 1)]
    async fn single_try(&self) -> RepositoryResult<u32> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(transient_error())
    }

    // Defaults apply (tries = 3); also proves arg-less usage parses.
    #[pgkit::retry(delay_ms = 1)]
    async fn with_defaults(&self) -> RepositoryResult<u32> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call < 3 {
            return Err(transient_error());
        }
        Ok(call)
    }

    #[pgkit::retry(tries = 3, backoff = "fixed", delay_ms = 1)]
    async fn ambiguous_failures(&self) -> RepositoryResult<u32> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call <= self.fail_times {
            return Err(ambiguous_error());
        }
        Ok(call)
    }

    #[pgkit::retry(tries = 3, backoff = "fixed", delay_ms = 1, idempotent, jitter)]
    async fn ambiguous_failures_idempotent(&self) -> RepositoryResult<u32> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call <= self.fail_times {
            return Err(ambiguous_error());
        }
        Ok(call)
    }
}

#[tokio::test]
async fn retries_transient_error_until_success() -> RepositoryResult<()> {
    let repo = FlakyRepo::new(2);
    let result = repo.transient_failures().await?;
    assert_eq!(result, 3);
    assert_eq!(repo.calls(), 3);
    Ok(())
}

#[tokio::test]
async fn gives_up_after_tries_and_returns_the_error() {
    let repo = FlakyRepo::new(u32::MAX);
    let result = repo.transient_failures().await;
    assert!(matches!(result, Err(RepositoryError::Sqlx { .. })));
    assert_eq!(repo.calls(), 3);
}

#[tokio::test]
async fn does_not_retry_permanent_errors() {
    let repo = FlakyRepo::new(0);
    let result = repo.permanent_failure().await;
    assert!(matches!(result, Err(RepositoryError::RowNotFound { .. })));
    assert_eq!(repo.calls(), 1);
}

#[tokio::test]
async fn single_try_never_retries() {
    let repo = FlakyRepo::new(0);
    let result = repo.single_try().await;
    assert!(result.is_err());
    assert_eq!(repo.calls(), 1);
}

#[tokio::test]
async fn defaults_allow_three_attempts() -> RepositoryResult<()> {
    let repo = FlakyRepo::new(0);
    assert_eq!(repo.with_defaults().await?, 3);
    Ok(())
}

#[tokio::test]
async fn ambiguous_failures_are_not_retried_by_default() {
    let repo = FlakyRepo::new(1);
    let result = repo.ambiguous_failures().await;
    assert!(matches!(result, Err(RepositoryError::Sqlx { .. })));
    assert_eq!(repo.calls(), 1);
}

#[tokio::test]
async fn idempotent_flag_unlocks_ambiguous_retries() -> RepositoryResult<()> {
    let repo = FlakyRepo::new(2);
    assert_eq!(repo.ambiguous_failures_idempotent().await?, 3);
    assert_eq!(repo.calls(), 3);
    Ok(())
}

mod async_trait_impls {
    use super::*;

    #[async_trait::async_trait]
    trait Repository: Send + Sync {
        async fn flaky(&self, id: i32) -> RepositoryResult<i32>;
        async fn always_missing(&self, id: i32) -> RepositoryResult<i32>;
    }

    struct PgLikeRepo {
        calls: AtomicU32,
        fail_times: u32,
    }

    #[async_trait::async_trait]
    impl Repository for PgLikeRepo {
        #[pgkit::retry(tries = 3, backoff = "fixed", delay_ms = 1)]
        async fn flaky(&self, id: i32) -> RepositoryResult<i32> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call <= self.fail_times {
                return Err(transient_error());
            }
            Ok(id)
        }

        #[pgkit::retry(tries = 3, backoff = "fixed", delay_ms = 1)]
        async fn always_missing(&self, id: i32) -> RepositoryResult<i32> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(RepositoryError::RowNotFound {
                message: format!("id {id} not found"),
            })
        }
    }

    #[tokio::test]
    async fn retries_inside_async_trait_methods() -> RepositoryResult<()> {
        let repo = PgLikeRepo {
            calls: AtomicU32::new(0),
            fail_times: 2,
        };
        // Through the trait object, like repos are used in services.
        let dyn_repo: &dyn Repository = &repo;
        assert_eq!(dyn_repo.flaky(42).await?, 42);
        assert_eq!(repo.calls.load(Ordering::SeqCst), 3);
        Ok(())
    }

    #[tokio::test]
    async fn gives_up_inside_async_trait_methods() {
        let repo = PgLikeRepo {
            calls: AtomicU32::new(0),
            fail_times: u32::MAX,
        };
        let result = repo.flaky(42).await;
        assert!(matches!(result, Err(RepositoryError::Sqlx { .. })));
        assert_eq!(repo.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn permanent_errors_pass_through_async_trait_methods() {
        let repo = PgLikeRepo {
            calls: AtomicU32::new(0),
            fail_times: 0,
        };
        let result = repo.always_missing(7).await;
        assert!(matches!(result, Err(RepositoryError::RowNotFound { .. })));
        assert_eq!(repo.calls.load(Ordering::SeqCst), 1);
    }
}

mod free_function {
    use super::*;

    static CALLS: AtomicU32 = AtomicU32::new(0);

    #[pgkit::retry(tries = 4, backoff = "linear", delay_ms = 1)]
    async fn fetch_by_id(id: i32, fail_times: &u32) -> RepositoryResult<i32> {
        let call = CALLS.fetch_add(1, Ordering::SeqCst) + 1;
        if call <= *fail_times {
            return Err(transient_error());
        }
        Ok(id)
    }

    #[tokio::test]
    async fn works_on_free_functions_with_borrowed_args() -> RepositoryResult<()> {
        let fail_times = 3;
        let id = fetch_by_id(42, &fail_times).await?;
        assert_eq!(id, 42);
        assert_eq!(CALLS.load(Ordering::SeqCst), 4);
        Ok(())
    }
}

mod generic_function {
    use super::*;

    #[pgkit::retry(tries = 2, backoff = "exponential", delay_ms = 1)]
    async fn echo<T: Clone + Send + Sync>(value: &T) -> RepositoryResult<T> {
        Ok(value.clone())
    }

    #[tokio::test]
    async fn preserves_generics() -> RepositoryResult<()> {
        assert_eq!(echo(&7_i64).await?, 7);
        assert_eq!(echo(&"hi".to_string()).await?, "hi");
        Ok(())
    }
}
