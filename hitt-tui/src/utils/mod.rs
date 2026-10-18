pub mod clipboard;
pub mod pretty_print;
pub mod timing;

use std::path::PathBuf;

/// Expand a leading `~` in a path string to the user's home directory.
pub fn expand_tilde(path_str: &str) -> PathBuf {
    if path_str.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return home.join(path_str.trim_start_matches("~/"));
        }
    }
    PathBuf::from(path_str)
}
