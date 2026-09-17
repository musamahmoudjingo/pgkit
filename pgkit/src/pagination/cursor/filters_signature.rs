use sha2::{Digest, Sha256};

use super::error::CursorPaginationError;

const SIGNATURE_HEX_LEN: usize = 16;

/// Computes a stable 16-char (64-bit) hex signature from the filters.
///
/// Coherence fingerprint, not a cryptographic hash: it only detects whether a
/// cursor's filters changed since it was minted, so 64 bits is ample and the
/// full SHA-256 width is intentionally truncated.
pub(crate) fn compute_filters_signature<F>(filters: &F) -> Result<String, CursorPaginationError>
where
    F: serde::Serialize + ?Sized,
{
    let json =
        serde_json::to_string(filters).map_err(|err| CursorPaginationError::InternalError {
            message: "Failed to serialize filters".to_string(),
            source: Box::new(err),
        })?;
    let digest = Sha256::digest(json.as_bytes());
    let mut hex = String::with_capacity(SIGNATURE_HEX_LEN);
    for byte in digest.iter().take(SIGNATURE_HEX_LEN / 2) {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Filters<'a> {
        region: Option<&'a str>,
        status: Option<&'a str>,
    }

    #[test]
    fn produces_16_char_hex() {
        let sig = compute_filters_signature(&Filters {
            region: Some("Europe"),
            status: Some("active"),
        })
        .unwrap();
        assert_eq!(sig.len(), SIGNATURE_HEX_LEN);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn is_deterministic() {
        let a = compute_filters_signature(&Filters {
            region: Some("Europe"),
            status: Some("active"),
        })
        .unwrap();
        let b = compute_filters_signature(&Filters {
            region: Some("Europe"),
            status: Some("active"),
        })
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_filters_produce_different_signatures() {
        let a = compute_filters_signature(&Filters {
            region: Some("Europe"),
            status: Some("active"),
        })
        .unwrap();
        let b = compute_filters_signature(&Filters {
            region: Some("Asia"),
            status: Some("active"),
        })
        .unwrap();
        assert_ne!(a, b);
    }
}
