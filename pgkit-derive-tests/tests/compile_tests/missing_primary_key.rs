use pgkit::PgKit;

#[derive(PgKit)]
#[pgkit(table_name = "demo")]
pub struct NoPk {
    pub id: i32,
    pub name: String,
}

fn main() {}
