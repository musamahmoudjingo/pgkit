use crate::pagination::defaults::MAX_PAGE_SIZE;

/// Default page number (1-indexed).
const DEFAULT_PAGE: u32 = 1;

/// Default page size when the caller does not specify one.
const DEFAULT_PAGE_SIZE: u32 = 20;

/// Offset-based pagination (page / page_size, classic `LIMIT` / `OFFSET`).
///
/// Construct via [`new`](Self::new); the fields are private so the clamping
/// there cannot be bypassed.
#[derive(Debug, Copy, Clone)]
pub struct OffsetPagination {
    /// 1-indexed page number.
    page: u32,
    /// Items per page.
    page_size: u32,
}

impl Default for OffsetPagination {
    fn default() -> Self {
        Self {
            page: DEFAULT_PAGE,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

impl OffsetPagination {
    /// Constructs a [`OffsetPagination`], clamping `page_size` to `MAX_PAGE_SIZE`.
    /// `page` is `max`-ed with `1` so callers cannot construct a 0-indexed page.
    pub fn new(page: u32, page_size: u32) -> Self {
        Self {
            page: page.max(1),
            page_size: page_size.clamp(1, MAX_PAGE_SIZE),
        }
    }

    /// Returns the 1-indexed page number.
    pub fn page(&self) -> u32 {
        self.page
    }

    /// Returns the items per page.
    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    /// Returns the SQL offset (0-indexed): `(page - 1) * page_size`.
    pub fn offset(&self) -> u64 {
        (self.page.saturating_sub(1) as u64) * (self.page_size as u64)
    }

    /// Returns the SQL limit (the page size).
    pub fn limit(&self) -> u32 {
        self.page_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_pagination_first_page_is_zero() {
        let p = OffsetPagination::new(1, 20);
        assert_eq!(p.offset(), 0);
        assert_eq!(p.limit(), 20);
    }

    #[test]
    fn offset_pagination_second_page() {
        let p = OffsetPagination::new(2, 20);
        assert_eq!(p.offset(), 20);
    }

    #[test]
    fn offset_pagination_clamps_page_size() {
        let p = OffsetPagination::new(1, 1_000);
        assert_eq!(p.limit(), MAX_PAGE_SIZE);
    }

    #[test]
    fn offset_pagination_zero_page_treated_as_first() {
        let p = OffsetPagination::new(0, 20);
        assert_eq!(p.page(), 1);
        assert_eq!(p.offset(), 0);
    }
}
