use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::{App, AppMode, FocusArea, ModalKind, NavMode, RequestTabKind};
use hitt_core::core::constants::SCROLL_DELTA;
use hitt_core::core::request::RequestBody;

use super::actions::cycle_environment;
use super::command_mode::handle_command_mode;
use super::editors::abandon_editing;
use super::insert_mode::enter_insert_mode;
use super::modal_handlers::{apply_theme_preview, handle_modal_mode};
use super::navigation::handle_chain_editor_mode;
use super::protocols::{cycle_protocol_method, scroll_stream_list};
use super::sidebar::{handle_sidebar_action, sidebar_len};

pub(super) async fn handle_mouse(app: &mut App, mouse: MouseEvent) -> Result<()> {
    let col = mouse.column;
    let row = mouse.row;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if overlay_open(app) {
                handle_overlay_click(app, col, row).await?;
            } else {
                handle_mouse_click(app, col, row);
            }
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let up = mouse.kind == MouseEventKind::ScrollUp;
            if overlay_open(app) {
                handle_overlay_scroll(app, col, row, up).await?;
            } else {
                handle_mouse_scroll(app, col, row, up);
            }
        }
        _ => {}
    }

    Ok(())
}

pub(super) fn hit_test(rect: Option<Rect>, col: u16, row: u16) -> bool {
    match rect {
        Some(r) => hit_test_rect(r, col, row),
        None => false,
    }
}

pub(super) fn hit_test_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Item index of the `(rect, index)` region under the cursor.
fn hit_item(items: &[(Rect, usize)], col: u16, row: u16) -> Option<usize> {
    items
        .iter()
        .find(|(rect, _)| hit_test_rect(*rect, col, row))
        .map(|(_, idx)| *idx)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

// ---------------------------------------------------------------------------
// Overlays: modals, command palette, chain runner
// ---------------------------------------------------------------------------

/// True while an overlay owns the mouse; the panels behind it ignore clicks.
fn overlay_open(app: &App) -> bool {
    matches!(
        app.mode,
        AppMode::Modal(_) | AppMode::Command | AppMode::ChainEditor
    )
}

/// Feed `key` to the open overlay's own key handler, so mouse actions behave
/// exactly like their keyboard equivalents.
async fn send_overlay_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match app.mode.clone() {
        AppMode::Modal(kind) => handle_modal_mode(app, key, &kind).await,
        AppMode::Command => handle_command_mode(app, key).await,
        AppMode::ChainEditor => handle_chain_editor_mode(app, key).await,
        _ => Ok(()),
    }
}

async fn handle_overlay_click(app: &mut App, col: u16, row: u16) -> Result<()> {
    // Not drawn yet: nothing on screen to click.
    let Some(area) = app.regions.overlay else {
        return Ok(());
    };
    if !hit_test_rect(area, col, row) {
        // A click outside closes the overlay, like Esc (Confirm: "No").
        let close = if matches!(app.mode, AppMode::Modal(ModalKind::Confirm(_))) {
            KeyCode::Char('n')
        } else {
            KeyCode::Esc
        };
        return send_overlay_key(app, key(close)).await;
    }

    let AppMode::Modal(kind) = app.mode.clone() else {
        return Ok(());
    };
    if let ModalKind::Confirm(_) = kind {
        if hit_test(app.regions.confirm_yes, col, row) {
            send_overlay_key(app, key(KeyCode::Char('y'))).await?;
        } else if hit_test(app.regions.confirm_no, col, row) {
            send_overlay_key(app, key(KeyCode::Char('n'))).await?;
        }
        return Ok(());
    }

    let Some(idx) = hit_item(&app.regions.modal_items, col, row) else {
        return Ok(());
    };
    match kind {
        ModalKind::Search => open_search_result(app, idx),
        ModalKind::ThemePicker => {
            // Select (previewing it), then apply like Enter.
            app.theme_picker_selected = idx;
            apply_theme_preview(app);
            send_overlay_key(app, key(KeyCode::Enter)).await?;
        }
        ModalKind::CollectionPicker => {
            app.collection_picker_selected = idx;
            send_overlay_key(app, key(KeyCode::Enter)).await?;
        }
        ModalKind::DiffSelector => {
            app.diff_state.selected = idx;
            send_overlay_key(app, key(KeyCode::Char(' '))).await?;
        }
        ModalKind::EnvironmentEdit => {
            if app.editor.env_form().is_none() {
                app.editor.env_selected = idx;
            }
        }
        ModalKind::Runner => {
            if let Some(run) = app.runner.run.as_mut() {
                run.selected = idx;
                run.follow = false;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Open search result `idx` in a new tab and close the search modal.
fn open_search_result(app: &mut App, idx: usize) {
    if idx >= app.search_results.len() {
        return;
    }
    app.search_selected = idx;
    // Same path as Enter: switches to an already-open tab instead of duplicating it.
    super::modal_handlers::open_search_result(app, idx);
}

async fn handle_overlay_scroll(app: &mut App, col: u16, row: u16, up: bool) -> Result<()> {
    if !hit_test(app.regions.overlay, col, row) {
        return Ok(());
    }
    let step = key(if up { KeyCode::Up } else { KeyCode::Down });
    match app.mode.clone() {
        AppMode::Modal(ModalKind::Help) => {
            // Rendering clamps the scroll to the help text.
            app.help_scroll = if up {
                app.help_scroll.saturating_sub(SCROLL_DELTA)
            } else {
                app.help_scroll.saturating_add(SCROLL_DELTA)
            };
        }
        AppMode::Modal(ModalKind::DiffSelector) if app.diff_state.view.is_some() => {
            for _ in 0..SCROLL_DELTA {
                send_overlay_key(app, step).await?;
            }
        }
        AppMode::Modal(ModalKind::EnvironmentEdit) => {
            // With a form open the arrows move between its fields instead.
            if app.editor.env_form().is_none() {
                send_overlay_key(app, step).await?;
            }
        }
        AppMode::Modal(
            ModalKind::DiffSelector
            | ModalKind::ThemePicker
            | ModalKind::CollectionPicker
            | ModalKind::Runner,
        )
        | AppMode::ChainEditor => send_overlay_key(app, step).await?,
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------

/// Leave insert mode before a click moves focus away from the text input
/// being edited: an open field form is cancelled, the body textarea is
/// committed (see `abandon_editing`).
fn leave_text_input(app: &mut App) {
    if app.mode == AppMode::Insert {
        abandon_editing(app);
        app.nav_mode = NavMode::Panel;
    }
}

/// True while a request-area editor (field form or body textarea) is open.
fn editing_request_body(app: &App) -> bool {
    app.mode == AppMode::Insert
        && app.focus == FocusArea::RequestBody
        && (app.editor.body.is_some() || app.editor.request_form().is_some())
}

fn focus_panel(app: &mut App, target: FocusArea) {
    app.navigate_to_panel(target);
    app.nav_mode = NavMode::Panel;
}

/// True if the click at (`col`, `row`) lands on the text input currently
/// being typed into (so it must not end the editing).
fn on_active_text_input(
    app: &App,
    regions: &crate::app::ClickableRegions,
    col: u16,
    row: u16,
) -> bool {
    if app.mode != AppMode::Insert {
        return false;
    }
    match app.focus {
        // The URL bar's rect also holds the method selector.
        FocusArea::UrlBar => {
            hit_test(regions.url_bar, col, row) && !hit_test(regions.method_selector, col, row)
        }
        // Table rows are separate targets: clicking one closes the form.
        FocusArea::RequestBody => {
            editing_request_body(app)
                && hit_test(regions.request_body, col, row)
                && hit_item(&regions.request_rows, col, row).is_none()
        }
        FocusArea::ResponseBody => hit_test(regions.ws_input_bar, col, row),
        _ => false,
    }
}

fn handle_mouse_click(app: &mut App, col: u16, row: u16) {
    let regions = app.regions.clone();

    // Any click outside the input being edited leaves insert mode first, also
    // on blank / non-interactive areas (status bar, empty tab strip…), so
    // typing never keeps going into a field that no longer looks focused.
    // Done before any focus / mode change (see `abandon_editing`).
    if !on_active_text_input(app, &regions, col, row) {
        leave_text_input(app);
    }

    // Header tab bar: one region per rendered tab label; a click elsewhere in
    // the bar does nothing.
    if let Some(idx) = hit_item(&regions.header_tabs, col, row) {
        app.switch_tab(idx);
        return;
    }
    if hit_test(regions.header_tab_bar, col, row) {
        return;
    }

    // Environment indicator: cycle like `e`.
    if hit_test(regions.env_selector, col, row) {
        cycle_environment(app);
        return;
    }

    // Method selector click (cycle method/protocol)
    if hit_test(regions.method_selector, col, row) {
        cycle_protocol_method(app.active_tab_mut());
        return;
    }

    // URL bar click: start typing in the URL.
    if hit_test(regions.url_bar, col, row) {
        if !(app.mode == AppMode::Insert && app.focus == FocusArea::UrlBar) {
            focus_panel(app, FocusArea::UrlBar);
            enter_insert_mode(app);
        }
        return;
    }

    // Request sub-tabs
    if let Some((_, kind)) = regions
        .request_tabs
        .iter()
        .find(|(rect, _)| hit_test_rect(*rect, col, row))
    {
        if app.active_tab().request_tab != *kind {
            app.active_tab_mut().request_tab = *kind;
            app.request_scroll = 0;
        }
        focus_panel(app, FocusArea::RequestTabs);
        return;
    }

    // Request table rows (params, headers, form body, assertions)
    if let Some(idx) = hit_item(&regions.request_rows, col, row) {
        if let Some(sel) = request_table_selection(app) {
            *sel = idx;
        }
        focus_panel(app, FocusArea::RequestBody);
        return;
    }

    // Request body area
    if hit_test(regions.request_body, col, row) {
        if !editing_request_body(app) {
            focus_panel(app, FocusArea::RequestBody);
        }
        return;
    }

    // Response sub-tabs
    if let Some((_, kind)) = regions
        .response_tabs
        .iter()
        .find(|(rect, _)| hit_test_rect(*rect, col, row))
    {
        if app.active_tab().response_tab != *kind {
            app.active_tab_mut().response_tab = *kind;
            app.response_scroll = 0;
        }
        focus_panel(app, FocusArea::ResponseTabs);
        return;
    }

    // WS input bar click -> type a WS message
    if hit_test(regions.ws_input_bar, col, row) {
        if !(app.mode == AppMode::Insert && app.focus == FocusArea::ResponseBody) {
            focus_panel(app, FocusArea::ResponseBody);
            enter_insert_mode(app);
        }
        return;
    }

    // Response body area
    if hit_test(regions.response_body, col, row) {
        focus_panel(app, FocusArea::ResponseBody);
        return;
    }

    // Sidebar section tabs (a new section starts at its first entry)
    if let Some((_, section)) = regions
        .sidebar_section_tabs
        .iter()
        .find(|(rect, _)| hit_test_rect(*rect, col, row))
    {
        if app.sidebar_state.section != *section {
            app.sidebar_state.section = *section;
            app.sidebar_state.selected = 0;
            app.sidebar_state.scroll_offset = 0;
        }
        focus_panel(app, FocusArea::Sidebar);
        return;
    }

    // Sidebar entries: select the entry and trigger its action
    // (expand/collapse collections, open requests).
    if let Some(idx) = hit_item(&regions.sidebar_items, col, row) {
        app.sidebar_state.selected = idx;
        focus_panel(app, FocusArea::Sidebar);
        handle_sidebar_action(app);
        return;
    }

    // Blank sidebar space: focus only.
    if hit_test(regions.sidebar, col, row) {
        focus_panel(app, FocusArea::Sidebar);
    }
}

/// Selected row of the table shown in the active request sub-tab, if any.
fn request_table_selection(app: &mut App) -> Option<&mut usize> {
    let editor = &mut app.editor;
    let tab = &app.tabs[app.active_tab];
    match tab.request_tab {
        RequestTabKind::Params => Some(&mut editor.params_selected),
        RequestTabKind::Headers => Some(&mut editor.headers_selected),
        RequestTabKind::Assertions => Some(&mut editor.assertions_selected),
        RequestTabKind::Body => match tab.request.body {
            Some(RequestBody::FormUrlEncoded(_) | RequestBody::FormData(_)) => {
                Some(&mut editor.form_body_selected)
            }
            _ => None,
        },
        RequestTabKind::Auth => None,
    }
}

/// Number of rows of the table shown in the active request sub-tab.
fn request_table_len(app: &App) -> usize {
    let request = &app.active_tab().request;
    match app.active_tab().request_tab {
        RequestTabKind::Params => request.params.len(),
        RequestTabKind::Headers => request.headers.len(),
        RequestTabKind::Assertions => request.assertions.len(),
        RequestTabKind::Body => match &request.body {
            Some(RequestBody::FormUrlEncoded(pairs) | RequestBody::FormData(pairs)) => pairs.len(),
            _ => 0,
        },
        RequestTabKind::Auth => 0,
    }
}

fn handle_mouse_scroll(app: &mut App, col: u16, row: u16, up: bool) {
    let regions = app.regions.clone();

    // Sidebar: move the selection, never past the last entry (rendering
    // scrolls the list to keep it visible).
    if hit_test(regions.sidebar, col, row) {
        let last = sidebar_len(app).saturating_sub(1);
        let state = &mut app.sidebar_state;
        state.selected = if up {
            state.selected.saturating_sub(1)
        } else {
            (state.selected + 1).min(last)
        };
        return;
    }

    // Response body (protocol-aware; rendering clamps the HTTP scroll).
    if hit_test(regions.response_body, col, row) {
        if !scroll_stream_list(app, SCROLL_DELTA, !up) {
            app.response_scroll = if up {
                app.response_scroll.saturating_sub(SCROLL_DELTA)
            } else {
                app.response_scroll.saturating_add(SCROLL_DELTA)
            };
        }
        return;
    }

    // Request body: the open textarea, else the table selection, else the
    // read-only text view.
    if hit_test(regions.request_body, col, row) {
        if editing_request_body(app) {
            if let Some(edit) = app.editor.body.as_mut() {
                let rows = i16::try_from(SCROLL_DELTA).unwrap_or(i16::MAX);
                edit.textarea.scroll((if up { -rows } else { rows }, 0));
            }
            return;
        }
        let len = request_table_len(app);
        if !regions.request_rows.is_empty() {
            if let Some(sel) = request_table_selection(app) {
                *sel = if up {
                    sel.saturating_sub(1)
                } else {
                    (*sel + 1).min(len.saturating_sub(1))
                };
            }
            return;
        }
        app.request_scroll = if up {
            app.request_scroll.saturating_sub(SCROLL_DELTA)
        } else {
            app.request_scroll.saturating_add(SCROLL_DELTA)
        };
    }
}
