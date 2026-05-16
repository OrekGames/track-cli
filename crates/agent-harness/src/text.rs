//! String helpers used across runners.

/// Truncate `s` to at most `max_bytes` bytes for display, appending `"..."` when
/// truncated. Uses [`str::floor_char_boundary`] so the cut never lands inside a
/// multi-byte UTF-8 codepoint — subprocess output containing non-ASCII text
/// would otherwise panic with `byte index N is not a char boundary`.
pub fn truncate_for_display(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let cut = s.floor_char_boundary(max_bytes);
    format!("{}...", &s[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_ascii_passes_through() {
        assert_eq!(truncate_for_display("hello", 100), "hello");
    }

    #[test]
    fn long_ascii_truncates_at_exact_byte() {
        let s = "a".repeat(500);
        let out = truncate_for_display(&s, 100);
        assert_eq!(out.len(), 103); // 100 bytes + "..."
        assert!(out.ends_with("..."));
    }

    #[test]
    fn does_not_panic_when_cut_lands_mid_codepoint() {
        // 499 ASCII bytes followed by "é" (2 bytes: 0xC3 0xA9).
        // Byte 500 is the second byte of "é" — without floor_char_boundary,
        // `&s[..500]` would panic.
        let mut s = "a".repeat(499);
        s.push('é');
        s.push_str(&"b".repeat(100)); // pad past the cut
        let out = truncate_for_display(&s, 500);
        // floor_char_boundary(500) walks back to 499 (the start of "é"),
        // so we get 499 'a's followed by "...".
        assert_eq!(out.len(), 502);
        assert!(out.ends_with("..."));
        // Critically: the result must be valid UTF-8 (implicit via `String`).
    }

    #[test]
    fn cut_just_after_multibyte_codepoint_includes_it() {
        // 498 ASCII bytes + "é" (2 bytes) = ends at byte 500.
        // floor_char_boundary(500) should return 500 — the boundary right after "é".
        let mut s = "a".repeat(498);
        s.push('é');
        s.push_str(&"b".repeat(100));
        let out = truncate_for_display(&s, 500);
        assert!(out.ends_with("é..."));
    }

    #[test]
    fn cjk_at_cut_does_not_panic() {
        // CJK codepoints are 3 bytes in UTF-8. Construct a string where the
        // requested cut lands inside one.
        let mut s = "a".repeat(298);
        s.push('完'); // 3 bytes
        s.push_str(&"b".repeat(100));
        let out = truncate_for_display(&s, 300);
        // floor_char_boundary walks back to 298. No panic, valid UTF-8.
        assert!(out.ends_with("..."));
        assert!(!out.contains('完')); // would have required including all 3 bytes
    }
}
