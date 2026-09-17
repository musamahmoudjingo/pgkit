//! Full-text search helpers for Postgres `tsvector` / `tsquery` columns.

/// The most terms one query may carry; extra terms are ignored. Postgres has
/// its own tsquery size limit past which `to_tsquery` raises a syntax error,
/// and every term costs CPU, so user-supplied search input should be bounded.
/// Sixteen is far past any real search.
pub const MAX_TERMS: usize = 16;

/// Builds a sanitized prefix-match `to_tsquery` string from a free-text search
/// term, e.g. `"red shoe"` → `"red:* & shoe:*"`. tsquery metacharacters are
/// stripped from each term so user input can never inject query operators, and
/// at most [`MAX_TERMS`] terms are used. Returns `None` when nothing usable
/// remains.
pub fn build_tsquery(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .filter_map(|term| {
            let sanitized: String = term
                .chars()
                .filter(|c| {
                    // `<`/`>` form the `<->` phrase operator and `\` escapes; all
                    // three make `to_tsquery` raise a syntax error.
                    !matches!(
                        c,
                        '&' | '|' | '!' | '(' | ')' | ':' | '*' | '\'' | '"' | '<' | '>' | '\\'
                    )
                })
                .collect();
            (!sanitized.is_empty()).then(|| format!("{sanitized}:*"))
        })
        .take(MAX_TERMS)
        .collect();
    (!terms.is_empty()).then(|| terms.join(" & "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tsquery_joins_prefix_terms() {
        assert_eq!(build_tsquery("red shoe").as_deref(), Some("red:* & shoe:*"));
        assert!(build_tsquery("   ").is_none());
    }

    #[test]
    fn build_tsquery_strips_tsquery_syntax_chars() {
        // Previously `a<b` reached to_tsquery verbatim and raised a syntax error.
        assert_eq!(build_tsquery("a<b").as_deref(), Some("ab:*"));
        assert_eq!(build_tsquery("a>b\\c").as_deref(), Some("abc:*"));
    }

    #[test]
    fn build_tsquery_caps_the_term_count() {
        let input = (0..100)
            .map(|i| format!("term{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let query = build_tsquery(&input).expect("terms remain");

        assert_eq!(query.matches(":*").count(), MAX_TERMS);
        assert!(query.starts_with("term0:*"));
    }

    #[test]
    fn build_tsquery_strips_metacharacters() {
        assert_eq!(build_tsquery("a&b !(c)").as_deref(), Some("ab:* & c:*"));
        assert!(build_tsquery("&|!():*'\"").is_none());
    }
}
