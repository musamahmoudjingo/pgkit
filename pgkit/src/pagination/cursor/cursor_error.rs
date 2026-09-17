/// Errors that can occur during cursor encode/decode.
#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    /// Base64 decoding failed.
    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),

    /// UTF-8 decoding failed.
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    /// JSON decoding failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The sort value carried by the cursor is of a type the sort column
    /// cannot hold, so no page of this listing could have minted it.
    ///
    /// Binding it anyway makes PostgreSQL fail with an operator/cast error,
    /// which is indistinguishable from a server fault; rejecting here keeps it
    /// a client error.
    #[error("cursor value {value} does not fit sort column of type {column}")]
    ValueTypeMismatch { value: &'static str, column: String },
}
