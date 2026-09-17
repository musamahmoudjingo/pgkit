use pgkit::PgKit;

#[derive(PgKit)]
#[pgkit(table_name = "demo")]
pub struct Tuple(pub i32, pub String);

fn main() {}
