use std::time::Duration;

/// Settings for [`get`](super::get), with production-safe defaults.
///
/// Construct it with struct-update syntax so unset fields keep their
/// defaults:
///
/// ```
/// use std::time::Duration;
/// use pgkit::pool::PoolConfig;
///
/// let config = PoolConfig {
///     max_connections: 20,
///     ..PoolConfig::default()
/// };
/// assert_eq!(config.statement_timeout, Duration::from_secs(30));
/// ```
///
/// The two session timeouts follow Postgres's own convention, a value of
/// `Duration::ZERO` disables that timeout. A migration runner, for example,
/// wants both off so a long migration is not killed mid-flight:
///
/// ```
/// use std::time::Duration;
/// use pgkit::pool::PoolConfig;
///
/// let config = PoolConfig {
///     statement_timeout: Duration::ZERO,
///     idle_tx_timeout: Duration::ZERO,
///     ..PoolConfig::default()
/// };
/// ```
#[derive(Debug)]
pub struct PoolConfig {
    /// Upper bound on open connections. Default `5`.
    ///
    /// [`get`](super::get) also keeps a small warm floor of idle
    /// connections: a quarter of this value, clamped to between 1 and 5.
    pub max_connections: u32,
    /// Kills any statement that runs longer than this (Postgres
    /// `statement_timeout`, set per session). `ZERO` disables it.
    /// Default 30 seconds.
    pub statement_timeout: Duration,
    /// Kills a session that sits idle inside an open transaction longer
    /// than this (Postgres `idle_in_transaction_session_timeout`), so an
    /// abandoned transaction cannot hold locks and a pooled connection.
    /// `ZERO` disables it. Default 60 seconds.
    pub idle_tx_timeout: Duration,
    /// How long a caller waits for a connection from the pool before
    /// erroring. Default 3 seconds.
    pub acquire_timeout: Duration,
    /// Closes a connection idle for this long, down to the warm floor.
    /// Default 10 minutes.
    pub idle_timeout: Duration,
    /// Closes any connection older than this. Default 30 minutes.
    pub max_lifetime: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 5,
            statement_timeout: Duration::from_secs(30),
            idle_tx_timeout: Duration::from_secs(60),
            acquire_timeout: Duration::from_secs(3),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
        }
    }
}
