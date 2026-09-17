use pgkit::PgKit;

#[derive(PgKit)]
#[pgkit(table_name = "demo")]
pub struct TwoPks {
    #[pgkit(primary_key)]
    pub id: i32,
    #[pgkit(primary_key)]
    pub other: i32,
}

fn main() {}
