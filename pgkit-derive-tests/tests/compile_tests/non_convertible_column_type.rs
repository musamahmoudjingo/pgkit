use pgkit::PgKit;

// `PaginatedRow::ordering_column_value` extracts a cursor value for *every*
// column, so each column's Rust type must be `Into<CursorValue>`. `Vec<u8>`
// (a BYTEA column) decodes fine for the partial row but has no
// `From<Vec<u8>> for CursorValue` impl; the emitted `PaginatedRow` arm
// cannot convert it, so the derive output fails to compile.
#[derive(PgKit)]
#[pgkit(table_name = "demo")]
pub struct Demo {
    #[pgkit(primary_key)]
    pub id: i32,
    pub data: Vec<u8>,
}

fn main() {}
