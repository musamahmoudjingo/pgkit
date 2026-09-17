use sqlx::postgres::{PgQueryResult, PgRow};
use sqlx::{Executor, Postgres, QueryBuilder};

pub struct BuildQuery {
    qb: QueryBuilder<Postgres>,
}

impl BuildQuery {
    pub(crate) fn new(qb: QueryBuilder<Postgres>) -> Self {
        Self { qb }
    }

    pub async fn execute<'e, 'c: 'e, E>(mut self, executor: E) -> Result<PgQueryResult, sqlx::Error>
    where
        E: Executor<'c, Database = Postgres>,
    {
        self.qb.build().execute(executor).await
    }

    pub async fn fetch_one<'e, 'c: 'e, E>(mut self, executor: E) -> Result<PgRow, sqlx::Error>
    where
        E: Executor<'c, Database = Postgres>,
    {
        self.qb.build().fetch_one(executor).await
    }

    pub async fn fetch_optional<'e, 'c: 'e, E>(
        mut self,
        executor: E,
    ) -> Result<Option<PgRow>, sqlx::Error>
    where
        E: Executor<'c, Database = Postgres>,
    {
        self.qb.build().fetch_optional(executor).await
    }

    pub async fn fetch_all<'e, 'c: 'e, E>(mut self, executor: E) -> Result<Vec<PgRow>, sqlx::Error>
    where
        E: Executor<'c, Database = Postgres>,
    {
        self.qb.build().fetch_all(executor).await
    }
}
