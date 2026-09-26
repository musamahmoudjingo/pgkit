use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

use super::config::PoolConfig;

/// Connects and returns a [`PgPool`] configured by [`PoolConfig`].
///
/// On top of the pool limits, every session gets Postgres's
/// `statement_timeout` and `idle_in_transaction_session_timeout`, so a
/// runaway query or an abandoned transaction cannot hold a pooled
/// connection forever.
#[tracing::instrument(skip(connect_options))]
pub async fn get(
    connect_options: PgConnectOptions,
    config: PoolConfig,
) -> Result<PgPool, sqlx::Error> {
    // Small warm floor, capped so tiny pools (max=1) stay valid.
    let min_connections = (config.max_connections / 4)
        .clamp(1, 5)
        .min(config.max_connections);

    let session = [
        (
            "statement_timeout",
            config.statement_timeout.as_millis().to_string(),
        ),
        (
            "idle_in_transaction_session_timeout",
            config.idle_tx_timeout.as_millis().to_string(),
        ),
    ];

    PgPoolOptions::new()
        .acquire_timeout(config.acquire_timeout)
        .max_connections(config.max_connections)
        .min_connections(min_connections)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .connect_with(connect_options.options(session))
        .await
}

#[cfg(test)]
mod tests {
    use super::{PoolConfig, get};
    use sqlx::Row;

    #[sqlx::test]
    async fn session_timeouts_are_applied(pool: sqlx::PgPool) -> Result<(), sqlx::Error> {
        let pool_config = PoolConfig {
            max_connections: 1,
            ..PoolConfig::default()
        };

        let options = (*pool.connect_options()).clone();
        let pool = get(options, pool_config).await?;

        let row = sqlx::query("SHOW statement_timeout")
            .fetch_one(&pool)
            .await?;
        assert_eq!(row.get::<String, _>(0), "30s");

        let row = sqlx::query("SHOW idle_in_transaction_session_timeout")
            .fetch_one(&pool)
            .await?;
        assert_eq!(row.get::<String, _>(0), "1min");
        Ok(())
    }
}
