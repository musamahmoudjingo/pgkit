use super::direction::OrderByDirection;

/// The order by criteria for a query.
#[derive(Debug, Clone)]
pub struct OrderBy<T> {
    by: T,
    dir: OrderByDirection,
}

impl<T> OrderBy<T> {
    /// Constructs a [`OrderBy`] with the specified column and direction.
    pub fn new(by: T, dir: OrderByDirection) -> Self {
        Self { by, dir }
    }

    /// Returns the sort direction.
    pub fn direction(&self) -> OrderByDirection {
        self.dir
    }
}

impl<T: Default> Default for OrderBy<T> {
    fn default() -> Self {
        Self {
            by: T::default(),
            dir: OrderByDirection::default(),
        }
    }
}

impl<T: Copy> OrderBy<T> {
    /// Returns the order-by column.
    pub fn column(&self) -> T {
        self.by
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    enum F {
        #[default]
        Name,
        CreatedAt,
    }

    #[test]
    fn column_falls_back_to_default() {
        let s: OrderBy<F> = OrderBy::default();
        assert_eq!(s.column(), F::Name);
    }

    #[test]
    fn direction_falls_back_to_default() {
        let s: OrderBy<F> = OrderBy::default();
        assert_eq!(s.direction(), OrderByDirection::Asc);
    }

    #[test]
    fn new_round_trips_field_and_direction() {
        let s = OrderBy::new(F::CreatedAt, OrderByDirection::Desc);
        assert_eq!(s.column(), F::CreatedAt);
        assert_eq!(s.direction(), OrderByDirection::Desc);
    }
}
