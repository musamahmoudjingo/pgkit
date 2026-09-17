use pgkit::PgKit;

#[derive(PgKit)]
#[pgkit(table_name = "demo", skip(columns))]
pub struct SkipsColumns {
    #[pgkit(primary_key)]
    pub id: i32,
}

fn main() {}
