//! `hitt-tui` — ratatui-based terminal UI and CLI for hitt.
//!
//! Depends on `hitt-core` for all domain logic. This crate only owns the
//! interactive presentation layer (TUI widgets, event loop, key bindings)
//! and the scriptable CLI front-end.

pub mod app;
pub mod cli;
pub mod editor;
pub mod event;
pub mod ui;
pub mod utils;
