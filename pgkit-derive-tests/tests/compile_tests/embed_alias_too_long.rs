use pgkit::PgKit;

#[derive(PgKit)]
#[pgkit(table_name = "comms_customer_notification_preferences")]
pub struct Pref {
    #[pgkit(primary_key)]
    pub id: i32,
    pub notification_template_rendering_language: String,
}

fn main() {}
