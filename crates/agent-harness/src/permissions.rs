//! Shared sanitization for tool-permission patterns given to coding agents.

/// Format the allowed Bash tool pattern by sanitizing `track_bin`.
///
/// Interpolating a raw binary path into patterns like `Bash(<path> *)` would
/// let hostile characters (`)`, `;`, `|`, newlines) break out of the pattern
/// and allow arbitrary commands. Stripping them keeps the pattern anchored to
/// the track binary.
pub(crate) fn format_allowed_bash_pattern(track_bin: &str) -> String {
    let sanitized: String = track_bin
        .chars()
        .filter(|c| !matches!(c, '(' | ')' | ';' | '&' | '|' | '\n' | '\r'))
        .collect();
    format!("Bash({} *)", sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_hostile_path_characters() {
        assert_eq!(
            format_allowed_bash_pattern("/usr/local/bin/track"),
            "Bash(/usr/local/bin/track *)"
        );
        assert_eq!(
            format_allowed_bash_pattern("/bin/track) || malicious_cmd"),
            "Bash(/bin/track  malicious_cmd *)"
        );
        assert_eq!(
            format_allowed_bash_pattern("/bin/track;\nevil & cmd"),
            "Bash(/bin/trackevil  cmd *)"
        );
        assert_eq!(
            format_allowed_bash_pattern("/bin/track && other | pipe"),
            "Bash(/bin/track  other  pipe *)"
        );
    }
}
