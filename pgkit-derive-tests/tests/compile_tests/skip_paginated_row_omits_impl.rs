use pgkit::PgKit;
use pgkit::pagination::cursor::PaginatedRow;

#[derive(PgKit)]
#[pgkit(table_name = "demo", skip(paginated_row))]
pub struct Demo {
    #[pgkit(primary_key)]
    pub id: i32,
    pub name: String,
}

// PartialDemo is emitted, but skip(paginated_row) means it does NOT
// implement PaginatedRow; this bound cannot be satisfied. PaginatedRow
// is parameterized by the full column enum (DemoColumn).
fn assert_paginated<T: PaginatedRow<i32, DemoColumn>>() {}

fn main() {
    assert_paginated::<PartialDemo>();
}
