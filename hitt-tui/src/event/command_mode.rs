use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppMode};

use super::commands::execute_command;
use super::is_text_input;

/// Most recent commands kept for Up/Down recall.
const COMMAND_HISTORY_LIMIT: usize = 100;

pub(super) async fn handle_command_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            // The nav mode from before `:` is restored by `handle_event`.
            app.mode = AppMode::Normal;
            app.command_input.clear();
            app.command_history_idx = None;
        }
        KeyCode::Enter => {
            let cmd = app.command_input.clone();
            app.mode = AppMode::Normal;
            app.command_input.clear();
            app.command_history_idx = None;
            // A bare `:` does nothing (like vim).
            if !cmd.trim().is_empty() {
                remember_command(app, cmd.trim());
                execute_command(app, &cmd).await?;
            }
        }
        KeyCode::Char(c) if is_text_input(key.modifiers) => {
            app.command_input.push(c);
        }
        KeyCode::Backspace => {
            // Like vim: deleting the last character leaves an empty `:`;
            // Backspace on an empty line leaves command mode.
            if app.command_input.pop().is_none() {
                app.mode = AppMode::Normal;
                app.command_history_idx = None;
            }
        }
        KeyCode::Up => recall_command(app, true),
        KeyCode::Down => recall_command(app, false),
        _ => {}
    }
    Ok(())
}

fn remember_command(app: &mut App, cmd: &str) {
    if app.command_history.last().map(String::as_str) != Some(cmd) {
        app.command_history.push(cmd.to_string());
    }
    if app.command_history.len() > COMMAND_HISTORY_LIMIT {
        app.command_history.remove(0);
    }
}

/// Up: older command; Down: newer command, then back to an empty line.
fn recall_command(app: &mut App, older: bool) {
    let len = app.command_history.len();
    if len == 0 {
        return;
    }
    let next = match (app.command_history_idx, older) {
        (None, true) => Some(len - 1),
        (Some(i), true) => Some(i.saturating_sub(1)),
        (Some(i), false) if i + 1 < len => Some(i + 1),
        (_, false) => None,
    };
    app.command_history_idx = next;
    app.command_input = next.map_or_else(String::new, |i| app.command_history[i].clone());
}
