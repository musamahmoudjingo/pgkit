/// How safe it is to re-execute the operation that produced an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrySafety {
    /// The outcome won't change on retry (constraint violations, missing
    /// rows, syntax errors, …).
    Never,
    /// The statement never executed or definitively rolled back (pool/connect
    /// rejections, deadlocks, serialization failures); safe to retry even
    /// for non-idempotent writes.
    Always,
    /// The failure happened mid-statement (I/O, TLS, connection dropped), so
    /// the database may have already applied the work. Retrying is only safe
    /// when the operation is idempotent.
    IfIdempotent,
}
