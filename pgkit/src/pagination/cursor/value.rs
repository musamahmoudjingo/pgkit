//! [`CursorValue`]: the typed sort-column value carried inside a cursor.
//!
//! Cursor (keyset) pagination resumes a listing "after" a known row. To do
//! that, the cursor must remember the last row's value for the column being
//! sorted on, carry it across the wire (the cursor is base64-encoded and
//! handed to the client), and feed it back into the next query's keyset
//! `WHERE`.
//!
//! `CursorValue` is that remembered value: a small, closed set of the
//! column types that can serve as a cursor sort key. Every variant has a
//! total SQL ordering and a faithful, round-trip encode/decode.

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgTypeInfo;
use sqlx::{Postgres, QueryBuilder, Type};
use uuid::Uuid;

/// A single, typed sort-column value as carried inside a pagination cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CursorValue {
    /// `BOOLEAN`.
    Bool(bool),

    /// 8-bit signed integer.
    TinyInt(i8),
    /// `SMALLINT`.
    SmallInt(i16),
    /// `INTEGER` / `INT` / `SERIAL`.
    Int(i32),
    /// `BIGINT` / `BIGSERIAL`.
    BigInt(i64),

    /// 8-bit unsigned integer.
    TinyUnsigned(u8),
    /// 16-bit unsigned integer.
    SmallUnsigned(u16),
    /// 32-bit unsigned integer.
    Unsigned(u32),
    /// 64-bit unsigned integer.
    ///
    /// Postgres has no unsigned 64-bit type, so this is bound as `BIGINT`
    /// (`i64`). Values ≥ 2^63 do **not** roundtrip; they wrap to negative and
    /// mis-order. Unreachable in practice (no Postgres column decodes to `u64`).
    BigUnsigned(u64),

    /// `REAL`.
    Float(f32),
    /// `DOUBLE PRECISION`.
    Double(f64),
    /// `NUMERIC` / `DECIMAL`.
    Decimal(Decimal),

    /// `TEXT` / `VARCHAR`.
    Text(String),

    /// `UUID`.
    Uuid(Uuid),

    /// `DATE`.
    Date(NaiveDate),
    /// `TIME`.
    Time(NaiveTime),
    /// `TIMESTAMP` (without time zone).
    DateTime(NaiveDateTime),
    /// `TIMESTAMPTZ` (timestamp with time zone).
    DateTimeUtc(DateTime<Utc>),

    /// `JSON` / `JSONB`.
    Json(serde_json::Value),

    /// A Postgres `enum`-typed column value, carried as its text label
    /// **only** (e.g. `"active"`).
    ///
    /// The Postgres type the label belongs to is **not** stored here; it is
    /// recovered server-side from the ordering column (via
    /// [`ColumnTypeInfo`](crate::types::ColumnTypeInfo)) when the keyset
    /// predicate is built. This is deliberate: the type name is structural
    /// SQL, and a cursor must never carry structure across the trust boundary
    /// (a client-supplied type name would be an injection vector). The keyset
    /// comparison casts the bound label to that server-derived type,
    /// `CAST($label AS <pg_type>)`, since `enum > text` is not a valid
    /// Postgres operator, and casting the column to text would mis-order (PG
    /// enums order by declaration order, not alphabetically).
    Enum(String),
}

// ---------------------------------------------------------------------------
// Inbound: a row's column value → `CursorValue`.
// ---------------------------------------------------------------------------

/// Generates `impl From<$ty> for CursorValue` for a scalar type that maps
/// directly onto a single `CursorValue` variant.
macro_rules! cursor_value_from {
    ($ty:ty => $variant:ident) => {
        impl From<$ty> for CursorValue {
            fn from(value: $ty) -> Self {
                CursorValue::$variant(value)
            }
        }
    };
}

cursor_value_from!(bool => Bool);
cursor_value_from!(i8 => TinyInt);
cursor_value_from!(i16 => SmallInt);
cursor_value_from!(i32 => Int);
cursor_value_from!(i64 => BigInt);
cursor_value_from!(u8 => TinyUnsigned);
cursor_value_from!(u16 => SmallUnsigned);
cursor_value_from!(u32 => Unsigned);
cursor_value_from!(u64 => BigUnsigned);
cursor_value_from!(f32 => Float);
cursor_value_from!(f64 => Double);
cursor_value_from!(Decimal => Decimal);
cursor_value_from!(String => Text);
cursor_value_from!(Uuid => Uuid);
cursor_value_from!(NaiveDate => Date);
cursor_value_from!(NaiveTime => Time);
cursor_value_from!(NaiveDateTime => DateTime);
cursor_value_from!(DateTime<Utc> => DateTimeUtc);
cursor_value_from!(serde_json::Value => Json);

// ---------------------------------------------------------------------------
// Outbound: push the value into a `sqlx::QueryBuilder` as a bound parameter.
// ---------------------------------------------------------------------------

impl CursorValue {
    /// Push `self` as a bound parameter into `sqlx::QueryBuilder`.
    pub fn push_bind_to(self, qb: &mut QueryBuilder<Postgres>) {
        match self {
            Self::Bool(v) => {
                qb.push_bind(v);
            }
            Self::TinyInt(v) => {
                qb.push_bind(v as i16);
            }
            Self::SmallInt(v) => {
                qb.push_bind(v);
            }
            Self::Int(v) => {
                qb.push_bind(v);
            }
            Self::BigInt(v) => {
                qb.push_bind(v);
            }
            Self::TinyUnsigned(v) => {
                qb.push_bind(v as i16);
            }
            Self::SmallUnsigned(v) => {
                qb.push_bind(v as i32);
            }
            Self::Unsigned(v) => {
                qb.push_bind(v as i64);
            }
            Self::BigUnsigned(v) => {
                debug_assert!(
                    v <= i64::MAX as u64,
                    "CursorValue::BigUnsigned({v}) ≥ 2^63 wraps to negative when bound as BIGINT"
                );
                qb.push_bind(v as i64);
            }
            Self::Float(v) => {
                qb.push_bind(v);
            }
            Self::Double(v) => {
                qb.push_bind(v);
            }
            Self::Decimal(v) => {
                qb.push_bind(v);
            }
            Self::Text(v) => {
                qb.push_bind(v);
            }
            Self::Uuid(v) => {
                qb.push_bind(v);
            }
            Self::Date(v) => {
                qb.push_bind(v);
            }
            Self::Time(v) => {
                qb.push_bind(v);
            }
            Self::DateTime(v) => {
                qb.push_bind(v);
            }
            Self::DateTimeUtc(v) => {
                qb.push_bind(v);
            }
            Self::Json(v) => {
                qb.push_bind(v);
            }
            Self::Enum(_) => unreachable!(
                "CursorValue::Enum is bound via the ordering column's CAST path \
                 (push_keyset_predicate), not through push_bind_to"
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Type checking: could this value have come from the column it claims?
// ---------------------------------------------------------------------------

/// The class of Postgres column type a [`CursorValue`] can be compared against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Bool,
    Numeric,
    Text,
    Uuid,
    Date,
    Time,
    Timestamp,
    Json,
    Enum,
}

impl ValueKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "boolean",
            Self::Numeric => "numeric",
            Self::Text => "text",
            Self::Uuid => "uuid",
            Self::Date => "date",
            Self::Time => "time",
            Self::Timestamp => "timestamp",
            Self::Json => "json",
            Self::Enum => "enum label",
        }
    }
}

fn is_type<T: Type<Postgres>>(ty: &PgTypeInfo) -> bool {
    *ty == <T as Type<Postgres>>::type_info()
}

/// Classifies a column's Postgres type, or `None` for one that maps to no
/// [`CursorValue`] variant at all (arrays, `bytea`, network types, …); those
/// are left unclassified rather than guessed at, so an unfamiliar column type
/// can never reject a cursor the library itself minted.
fn column_kind(ty: &PgTypeInfo) -> Option<ValueKind> {
    // Built-in types carry a static OID; a type sqlx knows only by name (a
    // Postgres `enum`, composite or domain) resolves one at runtime.
    if ty.oid().is_none() {
        return Some(ValueKind::Enum);
    }
    if is_type::<bool>(ty) {
        return Some(ValueKind::Bool);
    }
    if is_type::<i16>(ty)
        || is_type::<i32>(ty)
        || is_type::<i64>(ty)
        || is_type::<f32>(ty)
        || is_type::<f64>(ty)
        || is_type::<Decimal>(ty)
    {
        return Some(ValueKind::Numeric);
    }
    if <String as Type<Postgres>>::compatible(ty) {
        return Some(ValueKind::Text);
    }
    if is_type::<Uuid>(ty) {
        return Some(ValueKind::Uuid);
    }
    if is_type::<NaiveDate>(ty) {
        return Some(ValueKind::Date);
    }
    if is_type::<NaiveTime>(ty) {
        return Some(ValueKind::Time);
    }
    if is_type::<NaiveDateTime>(ty) || is_type::<DateTime<Utc>>(ty) {
        return Some(ValueKind::Timestamp);
    }
    if <serde_json::Value as Type<Postgres>>::compatible(ty) {
        return Some(ValueKind::Json);
    }
    None
}

impl CursorValue {
    fn kind(&self) -> ValueKind {
        match self {
            Self::Bool(_) => ValueKind::Bool,
            Self::TinyInt(_)
            | Self::SmallInt(_)
            | Self::Int(_)
            | Self::BigInt(_)
            | Self::TinyUnsigned(_)
            | Self::SmallUnsigned(_)
            | Self::Unsigned(_)
            | Self::BigUnsigned(_)
            | Self::Float(_)
            | Self::Double(_)
            | Self::Decimal(_) => ValueKind::Numeric,
            Self::Text(_) => ValueKind::Text,
            Self::Uuid(_) => ValueKind::Uuid,
            Self::Date(_) => ValueKind::Date,
            Self::Time(_) => ValueKind::Time,
            Self::DateTime(_) | Self::DateTimeUtc(_) => ValueKind::Timestamp,
            Self::Json(_) => ValueKind::Json,
            Self::Enum(_) => ValueKind::Enum,
        }
    }

    pub(crate) fn kind_name(&self) -> &'static str {
        self.kind().as_str()
    }

    /// Whether a column of `column_type` could have produced this value.
    ///
    /// A cursor is minted from the sort column's own Rust type, so a
    /// legitimate value always shares its column's class. A crafted or stale
    /// one need not, and binding it would make PostgreSQL fail with a cast or
    /// missing-operator error rather than a clean rejection.
    pub(crate) fn fits_column(&self, column_type: &PgTypeInfo) -> bool {
        column_kind(column_type).is_none_or(|kind| kind == self.kind())
    }
}
