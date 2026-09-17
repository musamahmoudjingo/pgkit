/// Database sort direction: maps to SQL `ASC` / `DESC`.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum OrderByDirection {
    /// Ascending order (oldest / smallest first). This is the default.
    #[default]
    Asc,
    /// Descending order (newest / largest first).
    Desc,
}

impl std::fmt::Display for OrderByDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Asc => f.write_str("ASC"),
            Self::Desc => f.write_str("DESC"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_asc() {
        assert_eq!(OrderByDirection::default(), OrderByDirection::Asc);
    }

    #[test]
    fn display_is_uppercase() {
        assert_eq!(OrderByDirection::Asc.to_string(), "ASC");
        assert_eq!(OrderByDirection::Desc.to_string(), "DESC");
    }
}
