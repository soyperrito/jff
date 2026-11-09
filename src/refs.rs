use chrono::{Local, NaiveDateTime, TimeDelta};

use crate::model::Kind;
use crate::text::normalize;

/// `[first letter][type][YYYYMMDDHHMMSS]`, e.g. `wt20261109123456`.
pub const STAMP_FORMAT: &str = "%Y%m%d%H%M%S";

pub fn timestamp_now() -> String {
    Local::now().format(STAMP_FORMAT).to_string()
}

/// Normalized first ASCII letter of a title, or `x` when there is none.
pub fn first_letter(title: &str) -> char {
    normalize(title)
        .chars()
        .find(|ch| ch.is_ascii_lowercase())
        .unwrap_or('x')
}

pub fn make(kind: Kind, title: &str, stamp: &str) -> String {
    format!("{}{}{}", first_letter(title), kind.as_char(), stamp)
}

/// Accepts the current 16-char refs and the 14-char refs from the original
/// specification (12 digits, minute precision).
pub fn is_valid(reference: &str) -> bool {
    let bytes = reference.as_bytes();

    matches!(bytes.len(), 14 | 16)
        && bytes[0].is_ascii_lowercase()
        && matches!(bytes[1], b't' | b'c')
        && bytes[2..].iter().all(u8::is_ascii_digit)
}

/// Advance a timestamp by one second. Used to keep refs unique when two
/// entries with the same initial letter are created in the same second.
pub fn bump_stamp(stamp: &str) -> String {
    NaiveDateTime::parse_from_str(stamp, STAMP_FORMAT)
        .map(|value| {
            (value + TimeDelta::seconds(1))
                .format(STAMP_FORMAT)
                .to_string()
        })
        .unwrap_or_else(|_| stamp.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_ref_from_title_and_kind() {
        assert_eq!(first_letter("Water"), 'w');
        assert_eq!(first_letter("Árbol"), 'a');
        assert_eq!(first_letter("123"), 'x');
        assert_eq!(
            make(Kind::Term, "Water", "20261109123456"),
            "wt20261109123456"
        );
        assert_eq!(
            make(Kind::Concept, "Árbol", "20261109123456"),
            "ac20261109123456"
        );
    }

    #[test]
    fn validates_current_and_legacy_refs() {
        assert!(is_valid("wt20261109123456"));
        assert!(is_valid("wc202611091234"));
        assert!(!is_valid("wt2026"));
        assert!(!is_valid("wz20261109123456"));
        assert!(!is_valid("wt2026110912345a"));
    }

    #[test]
    fn bumps_timestamp() {
        assert_eq!(bump_stamp("20261109123456"), "20261109123457");
        assert_eq!(bump_stamp("20261109123459"), "20261109123500");
    }
}
