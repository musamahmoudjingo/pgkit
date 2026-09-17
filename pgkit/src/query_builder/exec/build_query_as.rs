use std::marker::PhantomData;

use sqlx::postgres::PgRow;
use sqlx::{Executor, FromRow, Postgres, QueryBuilder};

pub struct BuildQueryAs<T> {
    qb: QueryBuilder<Postgres>,
    _row: PhantomData<fn() -> T>,
}

impl<T> BuildQueryAs<T> {
    pub(crate) fn new(qb: QueryBuilder<Postgres>) -> Self {
        Self {
            qb,
            _row: PhantomData,
        }
    }
}

impl<T> BuildQueryAs<T>
where
    T: Send + Unpin + for<'r> FromRow<'r, PgRow>,
{
    pub async fn fetch_one<'e, 'c: 'e, E>(mut self, executor: E) -> Result<T, sqlx::Error>
    where
        E: Executor<'c, Database = Postgres>,
    {
        self.qb.build_query_as::<T>().fetch_one(executor).await
    }

    pub async fn fetch_optional<'e, 'c: 'e, E>(
        mut self,
        executor: E,
    ) -> Result<Option<T>, sqlx::Error>
    where
        E: Executor<'c, Database = Postgres>,
    {
        self.qb.build_query_as::<T>().fetch_optional(executor).await
    }

    pub async fn fetch_all<'e, 'c: 'e, E>(mut self, executor: E) -> Result<Vec<T>, sqlx::Error>
    where
        E: Executor<'c, Database = Postgres>,
    {
        self.qb.build_query_as::<T>().fetch_all(executor).await
    }
}
