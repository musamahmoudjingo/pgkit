//! Repository error.
//!
//! This module defines the [`RepositoryError`] enum for database operation errors.
//! SQLx errors are automatically converted to appropriate variants via [`From<SqlxError>`],
//! extracting constraint names so callers can match on them.
//!
//! # Automatic Error Conversion
//!
//! When using `.map_err(RepositoryError::from)?` or the `?` operator with [`RepositoryResult`],
//! SQLx database errors are automatically categorized based on [`ErrorKind`]:
//!
//! | SQLx Error             | RepositoryError Variant   |
//! |------------------------|---------------------------|
//! | `UniqueViolation`      | `UniqueViolation`         |
//! | `ForeignKeyViolation`  | `ForeignKeyViolation`     |
//! | `CheckViolation`       | `CheckViolation`          |
//! | `NotNullViolation`     | `Required`                |
//! | `RowNotFound`          | `RowNotFound`             |
//! | Other                  | `Sqlx`                    |

use sqlx::Error as SqlxError;
use sqlx::error::ErrorKind;

use super::retry_safety::RetrySafety;
use crate::pagination::cursor::CursorPaginationError;

/// A type alias for repository results.
///
/// This alias simplifies the return type for repository methods by wrapping the
/// result in a `Result` with the `RepositoryError` type as the error variant.
pub type RepositoryResult<T> = std::result::Result<T, RepositoryError>;

/// Represents errors that can occur in the repository layer.
///
/// Database constraint violations are automatically categorized with extracted
/// constraint names so callers can match on them.
///
/// # Example
///
/// ```
/// # use pgkit::errors::{RepositoryError, RepositoryResult};
/// # struct Vendor;
/// # enum AppError { DuplicateName, Repository(RepositoryError) }
/// // In the calling code: match on a specific constraint
/// fn map_create_result(result: RepositoryResult<Vendor>) -> Result<Vendor, AppError> {
///     match result {
///         Ok(entity) => Ok(entity),
///         Err(RepositoryError::UniqueViolation { constraint: Some(c), .. })
///             if c == "uq_vendors_name" =>
///         {
///             Err(AppError::DuplicateName)
///         }
///         Err(e) => Err(AppError::Repository(e)),
///     }
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// Errors indicating the repository expected a non-empty payload
    /// but received an empty one (e.g. a PATCH update with no fields).
    #[error("payload was empty")]
    EmptyPayload,

    /// Check constraint violation (e.g., value out of allowed range).
    #[error("check constraint violation: {message}")]
    CheckViolation {
        /// The name of the violated constraint (e.g., "chk_customers_otp_messaging_app_requires_platform").
        constraint: Option<String>,
        /// The original PostgreSQL error message.
        message: String,
        /// The original SQLx error, if available.
        source: Option<SqlxError>,
    },

    /// Unique constraint violation (e.g., duplicate value in unique field).
    #[error("unique constraint violation: {message}")]
    UniqueViolation {
        /// The name of the violated constraint (e.g., "idx_vendor_unique_name").
        constraint: Option<String>,
        /// The original PostgreSQL error message.
        message: String,
        /// The original SQLx error, if available.
        source: Option<SqlxError>,
    },

    /// Foreign key constraint violation (e.g., referenced entity doesn't exist).
    #[error("foreign key violation: {message}")]
    ForeignKeyViolation {
        /// The name of the violated constraint (e.g., "fk_vendor_contact_persons_vendor").
        constraint: Option<String>,
        /// The original PostgreSQL error message.
        message: String,
        /// The original SQLx error, if available.
        source: Option<SqlxError>,
    },

    /// NOT NULL constraint violation (e.g., required field was null).
    #[error("required field missing: {message}")]
    Required {
        /// The original PostgreSQL error message.
        message: String,
        /// The original SQLx error, if available.
        source: Option<SqlxError>,
    },

    /// Error indicating no rows returned by a query that expected
    /// to return at least one row
    #[error("not found: {message}")]
    RowNotFound { message: String },

    /// A bound value could not be represented in the target SQL type
    /// (PostgreSQL SQLSTATE `22P02`, `invalid_text_representation`).
    ///
    /// The common cause is a tampered or stale pagination cursor carrying an
    /// enum label that is not a valid variant of the column's Postgres enum.
    #[error("invalid input value: {message}")]
    InvalidTextRepresentation {
        /// The original PostgreSQL message (kept for tracing, not for clients).
        message: String,
        /// The original SQLx error, if available.
        source: Option<SqlxError>,
    },

    /// All other SQLx-related errors (connection, query syntax, etc.).
    #[error("database error: {message}")]
    Sqlx {
        /// The original PostgreSQL error message.
        message: String,
        /// The original SQLx error, if available.
        source: Option<SqlxError>,
    },

    /// Errors that surface from cursor pagination (bad cursor, projection
    /// misconfiguration, etc.). These are not database errors; they're
    /// transported through `RepositoryError` so repository methods that call
    /// `CursorPagination::try_new` / `into_response_parts` can use `?`.
    /// Callers decide how to handle each variant.
    #[error("cursor pagination error: {0}")]
    CursorPagination(#[from] CursorPaginationError),

    /// Errors indicating an internal issue in the repository layer.
    #[error("{message}")]
    InternalError { message: String },
}

impl RepositoryError {
    /// Classifies how safe it is to re-execute the failed operation.
    ///
    /// Only [`Sqlx`](Self::Sqlx) errors can be retryable, and only when the
    /// underlying cause is transient; every other variant describes an
    /// outcome that won't change on retry.
    pub fn retry_safety(&self) -> RetrySafety {
        let RepositoryError::Sqlx {
            source: Some(source),
            ..
        } = self
        else {
            return RetrySafety::Never;
        };

        match source {
            // Acquiring a connection failed; nothing was ever sent.
            SqlxError::PoolTimedOut => RetrySafety::Always,
            // The connection died with a statement possibly in flight.
            SqlxError::Io(_) | SqlxError::Tls(_) | SqlxError::WorkerCrashed => {
                RetrySafety::IfIdempotent
            }
            SqlxError::Database(db_err) => db_err
                .code()
                .map_or(RetrySafety::Never, |code| transient_sqlstate_safety(&code)),
            _ => RetrySafety::Never,
        }
    }

    /// Whether the failed operation is worth retrying, given whether the
    /// caller asserts it is idempotent (safe to re-execute even if the
    /// previous attempt may have partially or fully applied).
    pub fn is_retryable(&self, idempotent: bool) -> bool {
        match self.retry_safety() {
            RetrySafety::Never => false,
            RetrySafety::Always => true,
            RetrySafety::IfIdempotent => idempotent,
        }
    }
}

/// Classifies Postgres SQLSTATEs that describe temporary conditions.
///
/// `Always`: the statement definitively did not apply:
/// - `40001`: serialization failure (transaction rolled back)
/// - `40P01`: deadlock detected (transaction rolled back)
/// - `53300`: too many connections (rejected at connect)
/// - `57P03`: cannot connect now (server starting up / shutting down)
///
/// `IfIdempotent`: `08…` connection exceptions: the link broke with a
/// statement possibly mid-execution, so the work may have applied.
fn transient_sqlstate_safety(code: &str) -> RetrySafety {
    if matches!(code, "40001" | "40P01" | "53300" | "57P03") {
        return RetrySafety::Always;
    }
    if code.starts_with("08") {
        return RetrySafety::IfIdempotent;
    }
    RetrySafety::Never
}

/// Converts SQLx errors to [`RepositoryError`] variants based on [`ErrorKind`].
///
/// Extracts the constraint name from database errors so callers can match on it.
/// Non-database errors (connection, protocol) become [`Sqlx`].
///
/// [`ErrorKind`]: sqlx::error::ErrorKind
/// [`Sqlx`]: RepositoryError::Sqlx
impl From<SqlxError> for RepositoryError {
    fn from(err: SqlxError) -> Self {
        match &err {
            SqlxError::Database(db_err) => {
                let constraint = db_err.constraint().map(String::from);
                let message = db_err.message().to_string();
                let is_invalid_text = db_err.code().as_deref() == Some("22P02");
                let kind = db_err.kind();

                // SQLSTATE 22P02 (invalid_text_representation): a bound value
                // could not be coerced to the target type.
                if is_invalid_text {
                    return RepositoryError::InvalidTextRepresentation {
                        message,
                        source: Some(err),
                    };
                }

                match kind {
                    ErrorKind::UniqueViolation => RepositoryError::UniqueViolation {
                        constraint,
                        message,
                        source: Some(err),
                    },
                    ErrorKind::ForeignKeyViolation => RepositoryError::ForeignKeyViolation {
                        constraint,
                        message,
                        source: Some(err),
                    },
                    ErrorKind::CheckViolation => RepositoryError::CheckViolation {
                        constraint,
                        message,
                        source: Some(err),
                    },
                    ErrorKind::NotNullViolation => RepositoryError::Required {
                        message,
                        source: Some(err),
                    },
                    _ => RepositoryError::Sqlx {
                        message,
                        source: Some(err),
                    },
                }
            }
            SqlxError::RowNotFound => RepositoryError::RowNotFound {
                message: "row not found".to_string(),
            },
            _ => RepositoryError::Sqlx {
                message: err.to_string(),
                source: Some(err),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlx_row_not_found_maps_to_row_not_found() {
        let err = RepositoryError::from(SqlxError::RowNotFound);
        assert!(matches!(err, RepositoryError::RowNotFound { .. }));
    }

    #[test]
    fn sqlx_non_database_error_maps_to_sqlx() {
        let err = RepositoryError::from(SqlxError::PoolTimedOut);
        match err {
            RepositoryError::Sqlx { source, .. } => assert!(source.is_some()),
            other => panic!("expected Sqlx, got {other:?}"),
        }
    }

    #[test]
    fn cursor_pagination_error_converts_via_from() {
        let err = RepositoryError::from(CursorPaginationError::IdColumnWasNotProjected);
        assert!(matches!(err, RepositoryError::CursorPagination(_)));
    }

    #[test]
    fn pool_timeout_is_always_retryable() {
        let pool = RepositoryError::from(SqlxError::PoolTimedOut);
        assert_eq!(pool.retry_safety(), RetrySafety::Always);
        assert!(pool.is_retryable(false));
        assert!(pool.is_retryable(true));
    }

    #[test]
    fn mid_statement_failures_require_idempotency() {
        let io = RepositoryError::from(SqlxError::Io(std::io::Error::other("reset")));
        assert_eq!(io.retry_safety(), RetrySafety::IfIdempotent);
        assert!(!io.is_retryable(false));
        assert!(io.is_retryable(true));

        let worker = RepositoryError::from(SqlxError::WorkerCrashed);
        assert_eq!(worker.retry_safety(), RetrySafety::IfIdempotent);
    }

    #[test]
    fn non_sqlx_variants_are_never_retryable() {
        for err in [
            RepositoryError::EmptyPayload,
            RepositoryError::RowNotFound {
                message: "x".into(),
            },
            RepositoryError::CheckViolation {
                constraint: None,
                message: "x".into(),
                source: None,
            },
            RepositoryError::ForeignKeyViolation {
                constraint: None,
                message: "x".into(),
                source: None,
            },
        ] {
            assert_eq!(err.retry_safety(), RetrySafety::Never);
            assert!(!err.is_retryable(true));
        }
    }

    #[test]
    fn sqlx_without_source_is_never_retryable() {
        let err = RepositoryError::Sqlx {
            message: "x".into(),
            source: None,
        };
        assert_eq!(err.retry_safety(), RetrySafety::Never);
    }

    #[test]
    fn pool_closed_is_never_retryable() {
        let err = RepositoryError::from(SqlxError::PoolClosed);
        assert_eq!(err.retry_safety(), RetrySafety::Never);
    }

    #[test]
    fn transient_sqlstates_are_classified() {
        for code in ["40001", "40P01", "53300", "57P03"] {
            assert_eq!(
                transient_sqlstate_safety(code),
                RetrySafety::Always,
                "{code}"
            );
        }
        for code in ["08000", "08006", "08001"] {
            assert_eq!(
                transient_sqlstate_safety(code),
                RetrySafety::IfIdempotent,
                "{code}"
            );
        }
        for code in ["23505", "23503", "22P02", "53100", "57P01", "42601"] {
            assert_eq!(
                transient_sqlstate_safety(code),
                RetrySafety::Never,
                "{code}"
            );
        }
    }
}
