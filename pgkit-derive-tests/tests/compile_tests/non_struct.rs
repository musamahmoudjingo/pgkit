use pgkit::PgKit;

#[derive(PgKit)]
#[pgkit(table_name = "demo")]
pub enum NotARow {
    A,
    B,
}

fn main() {}
