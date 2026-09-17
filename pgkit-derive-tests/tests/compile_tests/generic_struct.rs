use pgkit::PgKit;

#[derive(PgKit)]
#[pgkit(table_name = "demo")]
pub struct Generic<T> {
    #[pgkit(primary_key)]
    pub id: i32,
    pub payload: T,
}

fn main() {}
