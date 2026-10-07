use unicode_general_category::{GeneralCategory, get_general_category};
use uuid::Uuid;

pub const MAX_GROUP_NAME_LENGTH: usize = 24;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GroupType {
    #[default]
    Normal,
    Open,
    Isolated,
}

impl GroupType {
    #[must_use]
    pub const fn from_wire(value: i16) -> Self {
        match value {
            1 => Self::Open,
            2 => Self::Isolated,
            _ => Self::Normal,
        }
    }

    #[must_use]
    pub const fn to_wire(self) -> i16 {
        match self {
            Self::Normal => 0,
            Self::Open => 1,
            Self::Isolated => 2,
        }
    }

    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Open)
    }

    #[must_use]
    pub const fn is_isolated(self) -> bool {
        matches!(self, Self::Isolated)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub password: Option<String>,
    pub persistent: bool,
    pub hidden: bool,
    pub group_type: GroupType,
}

/// Matches Simple Voice Chat's `GROUP_REGEX`: 1-24 characters, no Unicode
/// `Other` (`\p{C}`) characters, and no leading ASCII whitespace. Also enforce
/// the wire format's UTF-16 limit, which is stricter than regex scalar counting.
#[must_use]
pub fn is_valid_group_text(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    !is_unicode_other(first)
        && !first.is_ascii_whitespace()
        && value.encode_utf16().count() <= MAX_GROUP_NAME_LENGTH
        && chars.all(|character| !is_unicode_other(character))
}

fn is_unicode_other(character: char) -> bool {
    matches!(
        get_general_category(character),
        GeneralCategory::Control
            | GeneralCategory::Format
            | GeneralCategory::PrivateUse
            | GeneralCategory::Surrogate
            | GeneralCategory::Unassigned
    )
}

#[cfg(test)]
mod tests {
    use super::{GroupType, is_valid_group_text};

    #[test]
    fn group_types_round_trip_the_protocol_values() {
        for value in 0..=2 {
            assert_eq!(GroupType::from_wire(value).to_wire(), value);
        }
        assert_eq!(GroupType::from_wire(-1), GroupType::Normal);
        assert_eq!(GroupType::from_wire(3), GroupType::Normal);
    }

    #[test]
    fn group_text_validation_matches_the_upstream_limits() {
        assert!(is_valid_group_text("Open group"));
        assert!(is_valid_group_text(&"a".repeat(24)));
        assert!(is_valid_group_text(&"😀".repeat(12)));
        assert!(!is_valid_group_text(&"😀".repeat(13)));
        assert!(is_valid_group_text(&format!("{}ab", "😀".repeat(11))));
        assert!(!is_valid_group_text(&format!("{}a", "😀".repeat(12))));
        assert!(!is_valid_group_text(""));
        assert!(!is_valid_group_text(" leading"));
        // Java's GROUP_REGEX does not enable UNICODE_CHARACTER_CLASS for \s.
        assert!(is_valid_group_text("\u{00a0}group"));
        assert!(is_valid_group_text("\u{3000}group"));
        assert!(!is_valid_group_text("line\nbreak"));
        assert!(!is_valid_group_text("zero\u{200b}width"));
        assert!(!is_valid_group_text("private\u{e000}use"));
        assert!(!is_valid_group_text(&"a".repeat(25)));
    }
}
