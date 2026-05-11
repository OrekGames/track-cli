//! String comparison helpers used across backends.
//!
//! `unicode_eq_ignore_case` is the Unicode-aware analogue of
//! [`str::eq_ignore_ascii_case`]. `case_key` produces a folded `String`
//! suitable for use as a `HashMap`/`HashSet` key when many comparisons
//! against the same value share an allocation.
//!
//! Use these whenever a comparison involves user input on one side and
//! tracker-supplied data (status names, custom field names, project names,
//! link types) on the other — those values can be localized and ASCII-only
//! case folding silently breaks all-caps non-English input.

/// Allocation-free Unicode case-insensitive equality.
///
/// Both inputs are folded with [`char::to_lowercase`] (the same full-Unicode
/// fold [`String::to_lowercase`] uses) and compared codepoint by codepoint.
/// Returns `true` when the folded sequences are identical.
pub fn unicode_eq_ignore_case(a: &str, b: &str) -> bool {
    let mut ai = a.chars().flat_map(char::to_lowercase);
    let mut bi = b.chars().flat_map(char::to_lowercase);
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x == y => continue,
            _ => return false,
        }
    }
}

/// Build a Unicode case-folded `String` suitable for use as a `HashMap` /
/// `HashSet` key. Use when the same value will be compared against many
/// candidates — folding once on insertion saves repeated folding on lookup.
///
/// Keys produced by this function compare equal exactly when
/// [`unicode_eq_ignore_case`] would return `true` for the originals.
pub fn case_key(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_case_insensitive() {
        assert!(unicode_eq_ignore_case("Foo", "FOO"));
        assert!(unicode_eq_ignore_case("foo", "FOO"));
        assert!(unicode_eq_ignore_case("foo", "foo"));
    }

    #[test]
    fn ascii_not_equal() {
        assert!(!unicode_eq_ignore_case("foo", "bar"));
        assert!(!unicode_eq_ignore_case("foo", "fooo"));
        assert!(!unicode_eq_ignore_case("fooo", "foo"));
    }

    #[test]
    fn german_umlaut() {
        assert!(unicode_eq_ignore_case("Geöffnet", "GEÖFFNET"));
        assert!(unicode_eq_ignore_case("Geöffnet", "geöffnet"));
        assert!(unicode_eq_ignore_case("GEÖFFNET", "geöffnet"));
    }

    #[test]
    fn french_accent() {
        assert!(unicode_eq_ignore_case("À faire", "à faire"));
        assert!(unicode_eq_ignore_case("À faire", "À FAIRE"));
        assert!(unicode_eq_ignore_case("à faire", "À FAIRE"));
    }

    #[test]
    fn cyrillic() {
        assert!(unicode_eq_ignore_case("Открыто", "открыто"));
        assert!(unicode_eq_ignore_case("Открыто", "ОТКРЫТО"));
    }

    #[test]
    fn czech_diacritic() {
        assert!(unicode_eq_ignore_case("Otevřeno", "OTEVŘENO"));
        assert!(unicode_eq_ignore_case("Otevřeno", "otevřeno"));
    }

    #[test]
    fn cjk_has_no_case() {
        assert!(unicode_eq_ignore_case("完了", "完了"));
        assert!(!unicode_eq_ignore_case("完了", "未対応"));
    }

    #[test]
    fn empty_strings() {
        assert!(unicode_eq_ignore_case("", ""));
        assert!(!unicode_eq_ignore_case("", "x"));
        assert!(!unicode_eq_ignore_case("x", ""));
    }

    #[test]
    fn case_key_matches_eq() {
        let pairs = [
            ("Foo", "FOO"),
            ("Geöffnet", "GEÖFFNET"),
            ("Открыто", "открыто"),
            ("完了", "完了"),
        ];
        for (a, b) in pairs {
            assert_eq!(
                case_key(a),
                case_key(b),
                "mismatched case_key for {a:?} / {b:?}"
            );
        }
    }

    #[test]
    fn case_key_distinguishes() {
        assert_ne!(case_key("foo"), case_key("bar"));
        assert_ne!(case_key("完了"), case_key("未対応"));
    }
}
