/// Default theme name used when no theme is configured.
pub const DEFAULT_THEME: &str = "catppuccin";

/// Default maximum number of history entries to keep.
pub const DEFAULT_HISTORY_LIMIT: usize = 1000;

/// Default HTTP request timeout in milliseconds (30 seconds).
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// TUI event loop tick rate in milliseconds.
pub const TUI_TICK_RATE_MS: u64 = 100;

/// Number of lines to scroll per mouse wheel tick or arrow key.
pub const SCROLL_DELTA: usize = 3;

/// Number of lines to scroll for half-page (J/K) navigation.
pub const HALF_PAGE_SCROLL: usize = 15;
