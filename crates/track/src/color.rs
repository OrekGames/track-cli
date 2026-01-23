use crate::cli::ColorChoice;
use std::io::IsTerminal;

/// Initialize color mode based on CLI choice and environment
pub fn init(choice: ColorChoice) {
    let should_color = match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            // Respect NO_COLOR standard (https://no-color.org/)
            if std::env::var("NO_COLOR").is_ok() {
                false
            } else {
                // Only colorize if stdout is a terminal
                std::io::stdout().is_terminal()
            }
        }
    };

    if should_color {
        colored::control::set_override(true);
    } else {
        colored::control::set_override(false);
    }
}
