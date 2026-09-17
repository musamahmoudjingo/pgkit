use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{
    CursorPaginationError, CursorValue, cursor_error::CursorError,
    filters_signature::compute_filters_signature,
};
use crate::ordering::OrderByDirection;

/// The pieces that make up a cursor.
///
/// Generic over `Col` (the sorting columns) and `Id` (primary key type; typically an integer or UUID).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPayload<Col, Id> {
    /// The SQL column name used for ordering (e.g., "created_at").
    pub col: Col,

    /// The ordering direction (ASC or DESC).
    pub dir: OrderByDirection,

    /// The ID of the last row in the current page.
    pub id: Id,

    /// The ordering column [`Self::col`]'s value for the last row of the
    /// current page. `None` when that row's sort column was SQL `NULL`.
    pub val: Option<CursorValue>,

    /// The signature of the filters that were used when this cursor was generated.
    pub filter_sig: Option<String>,
}

impl<Col, Id> CursorPayload<Col, Id> {
    /// Constructs a `CursorPayload`.
    ///
    /// # Args
    /// - `col`: The sorting column.
    /// - `dir`: The ordering direction (ASC or DESC).
    /// - `id`: The ID of the last row in the current page.
    /// - `val`: The value in the ordering column for the last row.
    /// - `filters`: The filters that were applied for the current page.
    ///
    /// # Errors
    /// Returns a [`CursorPaginationError`] if computing the filter signature fails.
    pub fn try_new<F>(
        col: Col,
        dir: OrderByDirection,
        id: Id,
        val: Option<CursorValue>,
        filters: Option<&F>,
    ) -> Result<Self, CursorPaginationError>
    where
        F: serde::Serialize + ?Sized,
    {
        let filter_sig = match filters {
            Some(f) => Some(compute_filters_signature(f)?),
            None => None,
        };
        Ok(Self {
            col,
            dir,
            id,
            val,
            filter_sig,
        })
    }

    /// Constructs a `CursorPayload` from an already-computed filter signature.
    pub fn with_filter_sig(
        col: Col,
        dir: OrderByDirection,
        id: Id,
        val: Option<CursorValue>,
        filter_sig: Option<&str>,
    ) -> Self {
        Self {
            col,
            dir,
            id,
            val,
            filter_sig: filter_sig.map(String::from),
        }
    }
}

impl<Col: Serialize, Id: Serialize> CursorPayload<Col, Id> {
    /// Serialize the payload into an opaque, URL-safe cursor string.
    ///
    /// # Errors
    /// Returns a [`CursorPaginationError`] if JSON serialization fails.
    pub fn encode(&self) -> Result<String, CursorPaginationError> {
        let json = serde_json::to_string(self).map_err(CursorError::from)?;
        Ok(BASE64_URL_SAFE_NO_PAD.encode(json.as_bytes()))
    }
}

impl<Col: DeserializeOwned, Id: DeserializeOwned> CursorPayload<Col, Id> {
    /// Decode an opaque cursor string back into a payload.
    ///
    /// # Errors
    /// Returns a [`CursorPaginationError`] if base64 decoding, UTF-8
    /// conversion, or JSON deserialization fails.
    pub fn decode(cursor: &str) -> Result<Self, CursorPaginationError> {
        let bytes = BASE64_URL_SAFE_NO_PAD
            .decode(cursor)
            .map_err(CursorError::Base64)?;
        let json = String::from_utf8(bytes).map_err(CursorError::Utf8)?;
        serde_json::from_str(&json)
            .map_err(CursorError::Json)
            .map_err(CursorPaginationError::from)
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    use super::super::cursor_error::CursorError;
    use super::{CursorPaginationError, CursorPayload, CursorValue};
    use crate::ordering::OrderByDirection;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    enum Columns {
        Name,
        CreatedAt,
    }

    #[test]
    fn encode_decode_roundtrip_uuid() -> Result<(), CursorPaginationError> {
        let id = Uuid::parse_str("018e3c8e-8b2a-7b4b-bb2a-1d2e3c4b5a6f").unwrap();
        let cursor = CursorPayload::with_filter_sig(
            Columns::CreatedAt,
            OrderByDirection::Desc,
            id,
            Some(CursorValue::Text("2024-01-15T10:30:00Z".to_string())),
            None,
        )
        .encode()?;

        let decoded = CursorPayload::<Columns, Uuid>::decode(&cursor)?;
        assert_eq!(decoded.col, Columns::CreatedAt);
        assert_eq!(decoded.dir, OrderByDirection::Desc);
        assert_eq!(
            decoded.val,
            Some(CursorValue::Text("2024-01-15T10:30:00Z".to_string()))
        );
        assert_eq!(decoded.id, id);

        Ok(())
    }

    #[test]
    fn encode_decode_roundtrip_i32() -> Result<(), CursorPaginationError> {
        let cursor = CursorPayload::with_filter_sig(
            Columns::Name,
            OrderByDirection::Asc,
            42_i32,
            Some(CursorValue::Text("Dubai".to_string())),
            None,
        )
        .encode()?;
        let decoded = CursorPayload::<Columns, i32>::decode(&cursor)?;
        assert_eq!(decoded.col, Columns::Name);
        assert_eq!(decoded.dir, OrderByDirection::Asc);
        assert_eq!(decoded.val, Some(CursorValue::Text("Dubai".to_string())));
        assert_eq!(decoded.id, 42);
        Ok(())
    }

    #[test]
    fn encode_decode_roundtrip_enum() -> Result<(), CursorPaginationError> {
        let cursor = CursorPayload::with_filter_sig(
            Columns::CreatedAt,
            OrderByDirection::Asc,
            7_i32,
            Some(CursorValue::Enum("active".to_string())),
            None,
        )
        .encode()?;
        let decoded = CursorPayload::<Columns, i32>::decode(&cursor)?;
        assert_eq!(decoded.col, Columns::CreatedAt);
        assert_eq!(decoded.dir, OrderByDirection::Asc);
        assert_eq!(decoded.val, Some(CursorValue::Enum("active".to_string())));
        assert_eq!(decoded.id, 7);
        Ok(())
    }

    #[test]
    fn decode_invalid_base64_returns_error() {
        let result = CursorPayload::<Columns, i32>::decode("not-valid-base64!!!");
        assert!(matches!(
            result,
            Err(CursorPaginationError::CursorError(CursorError::Base64(_)))
        ));
    }

    #[test]
    fn decode_invalid_utf8_returns_error() {
        let cursor = BASE64_URL_SAFE_NO_PAD.encode([0xFFu8, 0xFEu8]);
        let result = CursorPayload::<Columns, i32>::decode(&cursor);
        assert!(matches!(
            result,
            Err(CursorPaginationError::CursorError(CursorError::Utf8(_)))
        ));
    }

    #[test]
    fn decode_invalid_json_returns_error() {
        let cursor = BASE64_URL_SAFE_NO_PAD.encode(b"not json");
        let result = CursorPayload::<Columns, i32>::decode(&cursor);
        assert!(matches!(
            result,
            Err(CursorPaginationError::CursorError(CursorError::Json(_)))
        ));
    }

    #[test]
    fn decode_fails_when_id_type_mismatches() {
        // Encode with i32, decode as Uuid -> should fail at JSON deserialization.
        let cursor = CursorPayload::with_filter_sig(
            Columns::Name,
            OrderByDirection::Asc,
            42_i32,
            Some(CursorValue::Text("Dubai".to_string())),
            None,
        )
        .encode()
        .unwrap();
        let result = CursorPayload::<Columns, Uuid>::decode(&cursor);
        assert!(matches!(
            result,
            Err(CursorPaginationError::CursorError(CursorError::Json(_)))
        ));
    }
}
