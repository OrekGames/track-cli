use crate::cli::ColorChoice;
use std::ffi::OsStr;
use std::io::IsTerminal;

/// Initialize color mode based on CLI choice and environment
pub fn init(choice: ColorChoice) {
    colored::control::set_override(match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            if no_color_requested(std::env::var_os("NO_COLOR").as_deref()) {
                false
            } else {
                // Only colorize if stdout is a terminal
                std::io::stdout().is_terminal()
            }
        }
    });
}

/// Whether the `NO_COLOR` value disables color. Per https://no-color.org/ it
/// must be present and non-empty; any such value counts, even non-UTF-8.
fn no_color_requested(no_color: Option<&OsStr>) -> bool {
    no_color.is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_requires_a_non_empty_value() {
        assert!(no_color_requested(Some(OsStr::new("1"))));
        assert!(!no_color_requested(Some(OsStr::new(""))));
        assert!(!no_color_requested(None));
    }
}
