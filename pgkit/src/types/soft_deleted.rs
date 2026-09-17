#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IncludeSoftDeleted {
    Yes,
    #[default]
    No,
}

impl IncludeSoftDeleted {
    pub fn as_bool(&self) -> bool {
        matches!(self, IncludeSoftDeleted::Yes)
    }
}

impl From<bool> for IncludeSoftDeleted {
    fn from(value: bool) -> Self {
        if value {
            IncludeSoftDeleted::Yes
        } else {
            IncludeSoftDeleted::No
        }
    }
}
