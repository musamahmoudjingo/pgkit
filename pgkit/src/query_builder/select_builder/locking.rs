/// Row-locking clause attached to a SELECT (Postgres extension).
pub(super) enum Locking {
    Update,
    Share,
    NoKeyUpdate,
    KeyShare,
}

impl Locking {
    pub(super) fn keyword(&self) -> &'static str {
        match self {
            Self::Update => " FOR UPDATE",
            Self::Share => " FOR SHARE",
            Self::NoKeyUpdate => " FOR NO KEY UPDATE",
            Self::KeyShare => " FOR KEY SHARE",
        }
    }
}
