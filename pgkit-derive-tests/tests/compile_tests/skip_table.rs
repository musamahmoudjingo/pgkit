use pgkit::PgKit;

#[derive(PgKit)]
#[pgkit(table_name = "demo", skip(table))]
pub struct SkipsTable {
    #[pgkit(primary_key)]
    pub id: i32,
}

fn main() {}
