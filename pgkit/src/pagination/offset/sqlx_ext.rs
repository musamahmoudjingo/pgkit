//! Offset-pagination integration for [`sqlx::QueryBuilder`].
//!
//! Emits `LIMIT $n OFFSET $n` with the values bound, never interpolated.
//!
//! For [`crate::query_builder::SelectBuilder`], the equivalent `.paginate()`
//! method lives directly on the builder.

use sqlx::{Postgres, QueryBuilder};

use super::OffsetPagination;

/// Extension trait that adds `.paginate()` to [`sqlx::QueryBuilder`] for
/// offset-based pagination.
///
/// Appends ` LIMIT $n OFFSET $n` with both values bound as parameters.
pub trait SqlxOffsetPaginationExt {
    fn paginate(&mut self, pagination: &OffsetPagination) -> &mut Self;
}

impl SqlxOffsetPaginationExt for QueryBuilder<Postgres> {
    fn paginate(&mut self, pagination: &OffsetPagination) -> &mut Self {
        self.push(" LIMIT ");
        self.push_bind(i64::from(pagination.limit()));
        self.push(" OFFSET ");
        self.push_bind(pagination.offset() as i64);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pagination::defaults::MAX_PAGE_SIZE;
    use crate::query_builder::Query;
    use crate::query_builder::WhereOps;

    #[test]
    fn select_paginate_first_page() {
        let q = Query::select()
            .from("t")
            .column("id")
            .paginate(&OffsetPagination::new(1, 20))
            .build();
        assert_eq!(q.sql(), "SELECT id FROM t LIMIT $1 OFFSET $2");
    }

    #[test]
    fn select_paginate_second_page() {
        let q = Query::select()
            .from("t")
            .column("id")
            .paginate(&OffsetPagination::new(2, 20))
            .build();
        assert_eq!(q.sql(), "SELECT id FROM t LIMIT $1 OFFSET $2");
    }

    #[test]
    fn select_paginate_chains_with_where() {
        let q = Query::select()
            .from("t")
            .column("id")
            .where_null("deleted_at")
            .paginate(&OffsetPagination::new(2, 10))
            .build();
        assert_eq!(
            q.sql(),
            "SELECT id FROM t WHERE deleted_at IS NULL LIMIT $1 OFFSET $2"
        );
    }

    #[test]
    fn query_builder_paginate_appends_limit_offset() {
        let mut qb = sqlx::QueryBuilder::<Postgres>::new("SELECT id FROM t");
        qb.paginate(&OffsetPagination::new(2, 20));
        assert_eq!(qb.sql(), "SELECT id FROM t LIMIT $1 OFFSET $2");
    }

    #[test]
    fn query_builder_paginate_clamps_via_pagination_struct() {
        let mut qb = sqlx::QueryBuilder::<Postgres>::new("SELECT * FROM t");
        qb.paginate(&OffsetPagination::new(1, 10_000));
        // Page size is clamped inside OffsetPagination::new, but here we only
        // assert the SQL shape; values are bound, not inlined.
        assert_eq!(qb.sql(), "SELECT * FROM t LIMIT $1 OFFSET $2");
        // Smoke check that clamping happens upstream:
        assert_eq!(OffsetPagination::new(1, 10_000).limit(), MAX_PAGE_SIZE);
    }
}
