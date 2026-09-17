use pgkit::PgKit;

#[derive(PgKit)]
pub struct NoTableName {
    #[pgkit(primary_key)]
    pub id: i32,
}

fn main() {}
