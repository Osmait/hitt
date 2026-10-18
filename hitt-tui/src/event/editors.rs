//! Key handling for the request sub-tab editors (params, headers, body, auth,
//! assertions) and the environment variables modal.
//!
//! In normal mode the table keys (`a`, `x`/`d`, Space, Enter...) only apply
//! with Panel focus on the request area; once a field form or the body
//! textarea is open (insert mode) every printable key goes to it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, ModalKind, NavMode, NotificationKind, RequestTabKind};
use crate::editor::{
    apply_auth_values, assertion_fields, assertion_fields_for_kind, assertion_form_fields,
    assertion_form_kind, auth_form_fields, auth_rows, auth_type_name, body_text, body_type_label,
    build_assertion_kind, empty_env_var, format_json, is_form_body, is_text_body, json_error,
    next_auth_type, next_body_type, request_kv_rows_mut, set_body_text, BodyEdit, BodyTextTarget,
    EditTarget, FieldForm, FormField, KvRow, TextInput, VarScope,
};
use hitt_core::core::request::{KeyValuePair, RequestBody};
use hitt_core::testing::assertion_engine::Assertion;

use super::actions::cycle_environment;
use super::environments::{rename_environment, request_delete_environment};
use super::is_text_input;
use super::persistence::{save_all_collections, save_environment};

// ---------------------------------------------------------------------------
// Shared table navigation
// ---------------------------------------------------------------------------

enum TableOutcome {
    /// Not a table key: let the normal-mode handler have it.
    Ignored,
    /// Selection moved (nothing changed).
    Handled,
    /// Rows changed (delete / toggle).
    Changed,
    Add,
    Edit(usize),
}

/// j/k/g/G move, `a` add, `x`/`d`/Delete delete, Space toggle, Enter edit.
fn table_key<T>(
    rows: &mut Vec<T>,
    sel: &mut usize,
    key: KeyEvent,
    toggle: impl FnOnce(&mut T),
) -> TableOutcome {
    *sel = (*sel).min(rows.len().saturating_sub(1));
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char('j') | KeyCode::Down) => {
            if *sel + 1 < rows.len() {
                *sel += 1;
            }
            TableOutcome::Handled
        }
        (KeyModifiers::NONE, KeyCode::Char('k') | KeyCode::Up) => {
            *sel = sel.saturating_sub(1);
            TableOutcome::Handled
        }
        (KeyModifiers::NONE, KeyCode::Char('g') | KeyCode::Home) => {
            *sel = 0;
            TableOutcome::Handled
        }
        (m, KeyCode::Char('G')) if is_text_input(m) => {
            *sel = rows.len().saturating_sub(1);
            TableOutcome::Handled
        }
        (KeyModifiers::NONE, KeyCode::End) => {
            *sel = rows.len().saturating_sub(1);
            TableOutcome::Handled
        }
        (KeyModifiers::NONE, KeyCode::Char('a')) => TableOutcome::Add,
        (KeyModifiers::NONE, KeyCode::Char('x' | 'd') | KeyCode::Delete) => {
            if rows.is_empty() {
                return TableOutcome::Handled;
            }
            rows.remove(*sel);
            *sel = (*sel).min(rows.len().saturating_sub(1));
            TableOutcome::Changed
        }
        (KeyModifiers::NONE, KeyCode::Char(' ')) => match rows.get_mut(*sel) {
            Some(row) => {
                toggle(row);
                TableOutcome::Changed
            }
            None => TableOutcome::Handled,
        },
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if rows.is_empty() {
                TableOutcome::Add
            } else {
                TableOutcome::Edit(*sel)
            }
        }
        _ => TableOutcome::Ignored,
    }
}

fn kv_selection(app: &mut App, target: EditTarget) -> &mut usize {
    match target {
        EditTarget::Header => &mut app.editor.headers_selected,
        EditTarget::FormBody => &mut app.editor.form_body_selected,
        EditTarget::EnvVar | EditTarget::CollectionVar => &mut app.editor.env_selected,
        _ => &mut app.editor.params_selected,
    }
}

/// Open `form` and route keys to it. Request forms switch to insert mode;
/// environment forms stay inside their modal.
fn open_form(app: &mut App, form: FieldForm) {
    let in_modal = form.target.is_env_modal();
    app.editor.body = None;
    app.editor.form = Some(form);
    if !in_modal {
        app.nav_mode = NavMode::Panel;
        app.mode = AppMode::Insert;
    }
}

// ---------------------------------------------------------------------------
// Normal mode (request area focused, Panel nav)
// ---------------------------------------------------------------------------

/// Handle an editor key for the active request sub-tab. Returns false if the
/// key isn't an editor key, so the regular normal-mode bindings apply.
pub(super) fn handle_request_editor_key(app: &mut App, key: KeyEvent) -> bool {
    match app.active_tab().request_tab {
        RequestTabKind::Params => request_table_key(app, EditTarget::Param, key),
        RequestTabKind::Headers => request_table_key(app, EditTarget::Header, key),
        RequestTabKind::Body => body_key(app, key),
        RequestTabKind::Auth => auth_key(app, key),
        RequestTabKind::Assertions => assertions_key(app, key),
    }
}

fn request_table_key(app: &mut App, target: EditTarget, key: KeyEvent) -> bool {
    let idx = app.active_tab;
    let sel = match target {
        EditTarget::Header => &mut app.editor.headers_selected,
        EditTarget::FormBody => &mut app.editor.form_body_selected,
        _ => &mut app.editor.params_selected,
    };
    let Some(rows) = request_kv_rows_mut(&mut app.tabs[idx].request, target) else {
        return false;
    };
    match table_key(rows, sel, key, KeyValuePair::toggle_enabled) {
        TableOutcome::Ignored => return false,
        TableOutcome::Handled => {}
        TableOutcome::Changed => app.tabs[idx].dirty = true,
        TableOutcome::Add => start_request_row_add(app, target),
        TableOutcome::Edit(row) => start_request_row_edit(app, target, row),
    }
    true
}

fn start_request_row_add(app: &mut App, target: EditTarget) {
    let tab_id = app.active_tab().id;
    let idx = app.active_tab;
    let Some(rows) = request_kv_rows_mut(&mut app.tabs[idx].request, target) else {
        return;
    };
    rows.push(KeyValuePair::new("", ""));
    let row = rows.len() - 1;
    *kv_selection(app, target) = row;
    let mut form = FieldForm::key_value(target, tab_id, row, "", "");
    form.is_new = true;
    open_form(app, form);
}

fn start_request_row_edit(app: &mut App, target: EditTarget, row: usize) {
    let tab_id = app.active_tab().id;
    let idx = app.active_tab;
    let Some(pair) = request_kv_rows_mut(&mut app.tabs[idx].request, target)
        .and_then(|rows| rows.get(row))
        .cloned()
    else {
        return;
    };
    open_form(
        app,
        FieldForm::key_value(target, tab_id, row, &pair.key, &pair.value),
    );
}

fn body_key(app: &mut App, key: KeyEvent) -> bool {
    let body = app.active_tab().request.body.as_ref();
    let form_body = is_form_body(body);
    let is_json = matches!(body, Some(RequestBody::Json(_)));
    let is_raw = matches!(body, Some(RequestBody::Raw { .. }));
    let is_graphql = matches!(body, Some(RequestBody::GraphQL { .. }));

    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char('t')) => {
            let idx = app.active_tab;
            let tab = &mut app.tabs[idx];
            let stash = app.editor.body_stash.entry(tab.id).or_default();
            tab.request.body = next_body_type(tab.request.body.as_ref(), stash);
            tab.dirty = true;
            let label = body_type_label(tab.request.body.as_ref());
            app.editor.form_body_selected = 0;
            app.request_scroll = 0;
            app.notify(format!("Body type: {label}"), NotificationKind::Info);
            true
        }
        (KeyModifiers::NONE, KeyCode::Char('f')) if is_json => {
            format_json_body(app);
            true
        }
        (KeyModifiers::NONE, KeyCode::Char('c')) if is_raw => {
            let tab_id = app.active_tab().id;
            let current = match &app.active_tab().request.body {
                Some(RequestBody::Raw { content_type, .. }) => content_type.clone(),
                _ => String::new(),
            };
            open_form(
                app,
                FieldForm::new(
                    EditTarget::RawContentType,
                    tab_id,
                    vec![FormField::text("Content-Type", current)],
                ),
            );
            true
        }
        (m, KeyCode::Char('I')) if is_graphql && is_text_input(m) => {
            start_body_edit(app, BodyTextTarget::GraphqlVariables);
            true
        }
        (KeyModifiers::NONE, KeyCode::Enter) if !form_body => {
            begin_request_edit(app);
            true
        }
        _ if form_body => request_table_key(app, EditTarget::FormBody, key),
        // Text bodies keep the regular scroll keys.
        _ => false,
    }
}

fn format_json_body(app: &mut App) {
    let Some(RequestBody::Json(text)) = &app.active_tab().request.body else {
        return;
    };
    match format_json(text) {
        Ok(pretty) => {
            if pretty != *text {
                let tab = app.active_tab_mut();
                tab.request.body = Some(RequestBody::Json(pretty));
                tab.dirty = true;
            }
            app.notify("JSON formatted".into(), NotificationKind::Success);
        }
        Err(e) => app.notify(
            format!("Can't format, invalid JSON: {e}"),
            NotificationKind::Warning,
        ),
    }
}

fn auth_key(app: &mut App, key: KeyEvent) -> bool {
    let rows = auth_rows(app.active_tab().request.auth.as_ref()).len();
    app.editor.auth_selected = app.editor.auth_selected.min(rows.saturating_sub(1));
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char('t')) => {
            let tab = app.active_tab_mut();
            tab.request.auth = next_auth_type(tab.request.auth.as_ref());
            tab.dirty = true;
            let name = auth_type_name(tab.request.auth.as_ref());
            app.editor.auth_selected = 0;
            app.notify(format!("Auth: {name}"), NotificationKind::Info);
        }
        (KeyModifiers::NONE, KeyCode::Char('v')) => {
            app.editor.show_secrets = !app.editor.show_secrets;
        }
        (KeyModifiers::NONE, KeyCode::Char('j') | KeyCode::Down) => {
            if app.editor.auth_selected + 1 < rows {
                app.editor.auth_selected += 1;
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('k') | KeyCode::Up) => {
            app.editor.auth_selected = app.editor.auth_selected.saturating_sub(1);
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            begin_request_edit(app);
        }
        _ => return false,
    }
    true
}

fn assertions_key(app: &mut App, key: KeyEvent) -> bool {
    let idx = app.active_tab;
    let rows = &mut app.tabs[idx].request.assertions;
    match table_key(rows, &mut app.editor.assertions_selected, key, |a| {
        a.enabled = !a.enabled;
    }) {
        TableOutcome::Ignored => return false,
        TableOutcome::Handled => {}
        TableOutcome::Changed => app.tabs[idx].dirty = true,
        TableOutcome::Add => start_assertion_form(app, None),
        TableOutcome::Edit(row) => start_assertion_form(app, Some(row)),
    }
    true
}

fn start_assertion_form(app: &mut App, row: Option<usize>) {
    let tab_id = app.active_tab().id;
    let fields = match row.and_then(|r| app.active_tab().request.assertions.get(r)) {
        Some(a) => assertion_form_fields(&a.kind),
        // New assertions default to `status equals 200`.
        None => assertion_fields(0, &["200".to_string()]),
    };
    let mut form = FieldForm::new(EditTarget::Assertion, tab_id, fields);
    form.row = row;
    // Start on the first parameter; the kind is one Up/Shift+Tab away.
    form.focused = 1;
    open_form(app, form);
}

fn start_body_edit(app: &mut App, target: BodyTextTarget) {
    let tab = app.active_tab();
    let Some(text) = body_text(tab.request.body.as_ref(), target) else {
        return;
    };
    let tab_id = tab.id;
    app.editor.form = None;
    app.editor.body = Some(BodyEdit::new(tab_id, target, &text));
    app.nav_mode = NavMode::Panel;
    app.mode = AppMode::Insert;
}

/// Start editing whatever the active request sub-tab shows (`i` / Enter).
/// Returns false if there is nothing to edit (the user was told why).
pub(super) fn begin_request_edit(app: &mut App) -> bool {
    let tab = app.active_tab();
    match tab.request_tab {
        RequestTabKind::Params | RequestTabKind::Headers => {
            let target = if tab.request_tab == RequestTabKind::Params {
                EditTarget::Param
            } else {
                EditTarget::Header
            };
            start_table_edit(app, target);
        }
        RequestTabKind::Body => {
            let body = tab.request.body.as_ref();
            if is_text_body(body) {
                start_body_edit(app, BodyTextTarget::Content);
            } else if is_form_body(body) {
                start_table_edit(app, EditTarget::FormBody);
            } else {
                app.notify(
                    "No editable body: press t to choose a body type".into(),
                    NotificationKind::Warning,
                );
                return false;
            }
        }
        RequestTabKind::Auth => {
            let fields = auth_form_fields(tab.request.auth.as_ref());
            if fields.is_empty() {
                app.notify(
                    "Nothing to edit: press t to choose an auth type".into(),
                    NotificationKind::Warning,
                );
                return false;
            }
            let mut form = FieldForm::new(EditTarget::Auth, tab.id, fields);
            form.focused = app.editor.auth_selected.min(form.fields.len() - 1);
            open_form(app, form);
        }
        RequestTabKind::Assertions => {
            let len = tab.request.assertions.len();
            if len == 0 {
                start_assertion_form(app, None);
            } else {
                let row = app.editor.assertions_selected.min(len - 1);
                start_assertion_form(app, Some(row));
            }
        }
    }
    true
}

/// Edit the selected row, or add one if the table is empty.
fn start_table_edit(app: &mut App, target: EditTarget) {
    let idx = app.active_tab;
    let len = request_kv_rows_mut(&mut app.tabs[idx].request, target).map_or(0, |r| r.len());
    if len == 0 {
        start_request_row_add(app, target);
    } else {
        let row = (*kv_selection(app, target)).min(len - 1);
        start_request_row_edit(app, target, row);
    }
}

// ---------------------------------------------------------------------------
// Insert mode (form / textarea open)
// ---------------------------------------------------------------------------

/// Route an insert-mode key to the open body textarea or request form.
/// Returns false if no request editor is open.
pub(super) fn handle_request_editor_insert(app: &mut App, key: KeyEvent) -> bool {
    // Focus moved away (e.g. a click) while an editor was open: close it so
    // no blank row or stale form is left behind, and let the key go to the
    // newly focused input.
    if app.focus != crate::app::FocusArea::RequestBody {
        close_request_editors(app);
        return false;
    }
    if app.editor.body.is_some() {
        if key.code == KeyCode::Esc {
            finish_body_edit(app);
        } else if let Some(edit) = app.editor.body.as_mut() {
            edit.textarea.input(key);
        }
        return true;
    }
    if app.editor.request_form().is_some() {
        form_key(app, key);
        return true;
    }
    false
}

/// Stop editing whatever is open, as if focus moved elsewhere: an open field
/// form is cancelled (a row added for it is removed again, edits to an
/// existing row are discarded), the body textarea is committed (typed text is
/// kept, like Esc) and insert mode is left, including URL typing.
///
/// Call it *before* applying a focus / mode change that bypasses the editors'
/// own keys (mouse clicks, tab switches). Environment-modal forms are
/// cancelled too, but the modal itself stays open.
pub fn abandon_editing(app: &mut App) {
    close_request_editors(app);
    if app.editor.env_form().is_some() {
        cancel_form(app);
    }
    app.editor.url_cursor = None;
    app.editor.url_scroll = 0;
    if app.mode == AppMode::Insert {
        app.mode = AppMode::Normal;
    }
}

/// Cancel an open request-panel form and commit the body textarea, without
/// touching the app mode.
fn close_request_editors(app: &mut App) {
    if app.editor.request_form().is_some() {
        if let Some(form) = app.editor.form.take() {
            if form.is_new {
                if let Some(row) = form.row {
                    remove_row(app, &form, row);
                }
            }
        }
    }
    commit_body_edit(app);
}

/// Write the textarea back to its tab and leave insert mode.
fn finish_body_edit(app: &mut App) {
    app.mode = AppMode::Normal;
    commit_body_edit(app);
}

/// Write the textarea (if open) back to its tab, warning about invalid JSON.
fn commit_body_edit(app: &mut App) {
    let Some(edit) = app.editor.body.take() else {
        return;
    };
    let text = edit.text();
    let Some(tab) = app.tabs.iter_mut().find(|t| t.id == edit.tab_id) else {
        return;
    };
    if set_body_text(tab.request.body.as_mut(), edit.target, text.clone()) {
        tab.dirty = true;
    }
    let json_text = match (&tab.request.body, edit.target) {
        (Some(RequestBody::Json(_)), _)
        | (Some(RequestBody::GraphQL { .. }), BodyTextTarget::GraphqlVariables) => Some(text),
        _ => None,
    };
    if let Some(err) = json_text.as_deref().and_then(json_error) {
        app.notify(format!("Invalid JSON: {err}"), NotificationKind::Warning);
    }
}

/// Keys for an open field form: Tab/Shift+Tab/Up/Down switch fields,
/// Left/Right/Space cycle choices, Enter saves, Esc cancels.
fn form_key(app: &mut App, key: KeyEvent) {
    let Some(form) = app.editor.form.as_mut() else {
        return;
    };
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => cancel_form(app),
        (KeyModifiers::NONE, KeyCode::Enter) => match commit_form(app) {
            Ok(()) => close_form(app),
            Err(e) => {
                if let Some(form) = app.editor.form.as_mut() {
                    form.error = Some(e);
                }
            }
        },
        (KeyModifiers::NONE, KeyCode::Tab | KeyCode::Down) => form.focus_next(),
        (_, KeyCode::BackTab) | (KeyModifiers::NONE, KeyCode::Up) => form.focus_prev(),
        _ => {
            let focused = form.focused;
            let is_assertion = form.target == EditTarget::Assertion;
            let Some(field) = form.focused_field_mut() else {
                return;
            };
            if field.is_choice() {
                let forward = match key.code {
                    KeyCode::Left | KeyCode::Char('h') => false,
                    KeyCode::Right | KeyCode::Char('l' | ' ') => true,
                    _ => return,
                };
                field.cycle(forward);
                // Changing the assertion kind swaps the parameter fields.
                if is_assertion && focused == 0 {
                    form.fields =
                        assertion_fields_for_kind(assertion_form_kind(form), &form.fields);
                }
            } else {
                let input = &mut field.input;
                match (key.modifiers, key.code) {
                    (m, KeyCode::Char(c)) if is_text_input(m) => input.insert_char(c),
                    (_, KeyCode::Backspace) => input.backspace(),
                    (_, KeyCode::Delete) => input.delete(),
                    (_, KeyCode::Left) => input.left(),
                    (_, KeyCode::Right) => input.right(),
                    (_, KeyCode::Home) | (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                        input.home();
                    }
                    (_, KeyCode::End) | (KeyModifiers::CONTROL, KeyCode::Char('e')) => input.end(),
                    (KeyModifiers::CONTROL, KeyCode::Char('u')) => input.clear_to_start(),
                    _ => return,
                }
            }
            form.error = None;
        }
    }
}

fn close_form(app: &mut App) {
    let in_modal = app
        .editor
        .form
        .as_ref()
        .is_some_and(|f| f.target.is_env_modal());
    app.editor.form = None;
    if !in_modal {
        app.mode = AppMode::Normal;
    }
}

/// Drop the form; a row added for it is removed again.
fn cancel_form(app: &mut App) {
    if let Some(form) = app.editor.form.clone() {
        if form.is_new {
            if let Some(row) = form.row {
                remove_row(app, &form, row);
            }
        }
    }
    close_form(app);
}

fn remove_row(app: &mut App, form: &FieldForm, row: usize) {
    match form.target {
        EditTarget::EnvVar => {
            if let Some(env) = app.active_env.and_then(|i| app.environments.get_mut(i)) {
                if row < env.values.len() {
                    env.values.remove(row);
                }
            }
        }
        EditTarget::CollectionVar => {
            if let Some(coll) = active_collection_mut(app) {
                if row < coll.variables.len() {
                    coll.variables.remove(row);
                }
            }
        }
        target => {
            if let Some(rows) = app
                .tabs
                .iter_mut()
                .find(|t| t.id == form.tab_id)
                .and_then(|t| request_kv_rows_mut(&mut t.request, target))
            {
                if row < rows.len() {
                    rows.remove(row);
                }
            }
        }
    }
    // Added rows are always last, so the selection moves up one.
    let sel = kv_selection(app, form.target);
    if *sel >= row {
        *sel = sel.saturating_sub(1);
    }
}

/// Write a key/value form into `rows[row]`. A new row left blank is removed.
fn commit_kv<R: KvRow>(rows: &mut Vec<R>, form: &FieldForm, row: usize) -> Result<bool, String> {
    let values = form.values();
    let key = values.first().map_or("", |k| k.trim()).to_string();
    let value = values.get(1).cloned().unwrap_or_default();
    if key.is_empty() {
        if form.is_new && value.is_empty() {
            if row < rows.len() {
                rows.remove(row);
            }
            return Ok(false);
        }
        return Err("Key cannot be empty".into());
    }
    let Some(target) = rows.get_mut(row) else {
        return Err("Row no longer exists".into());
    };
    if target.row_key() == key && target.row_value() == value {
        return Ok(false);
    }
    target.set_key_value(key, value);
    Ok(true)
}

fn commit_form(app: &mut App) -> Result<(), String> {
    let Some(form) = app.editor.form.clone() else {
        return Ok(());
    };
    let values = form.values();
    match form.target {
        EditTarget::EnvVar => {
            let idx = app.active_env.ok_or("No active environment")?;
            let env = app
                .environments
                .get_mut(idx)
                .ok_or("No active environment")?;
            if commit_kv(&mut env.values, &form, form.row.unwrap_or(0))? {
                persist_vars(app, VarScope::Environment);
            }
            return Ok(());
        }
        EditTarget::CollectionVar => {
            let coll = active_collection_mut(app).ok_or("No collection for this tab")?;
            if commit_kv(&mut coll.variables, &form, form.row.unwrap_or(0))? {
                persist_vars(app, VarScope::Collection);
            }
            return Ok(());
        }
        EditTarget::NewEnvironment => {
            let name = values.first().map_or("", |n| n.trim()).to_string();
            if name.is_empty() {
                return Err("Name is required".into());
            }
            if app.environments.iter().any(|e| e.name == name) {
                return Err(format!("Environment '{name}' already exists"));
            }
            app.environments
                .push(hitt_core::core::environment::Environment::new(&name));
            let idx = app.environments.len() - 1;
            app.active_env = Some(idx);
            app.remember_active_env();
            app.editor.env_scope = VarScope::Environment;
            app.editor.env_selected = 0;
            save_environment(app, idx);
            app.notify(
                format!("Created environment '{name}'"),
                NotificationKind::Success,
            );
            return Ok(());
        }
        EditTarget::RenameEnvironment => {
            let idx = app.active_env.ok_or("No active environment")?;
            let name = values.first().map_or("", |n| n.trim()).to_string();
            return rename_environment(app, idx, &name);
        }
        _ => {}
    }

    let tab = app
        .tabs
        .iter_mut()
        .find(|t| t.id == form.tab_id)
        .ok_or("The tab was closed")?;
    let changed = match form.target {
        EditTarget::Param | EditTarget::Header | EditTarget::FormBody => {
            let rows = request_kv_rows_mut(&mut tab.request, form.target)
                .ok_or("The body type changed")?;
            commit_kv(rows, &form, form.row.unwrap_or(0))?
        }
        EditTarget::Auth => {
            let auth = tab.request.auth.as_mut().ok_or("No auth configured")?;
            apply_auth_values(auth, &values);
            true
        }
        EditTarget::Assertion => {
            let kind = build_assertion_kind(&values)?;
            let assertions = &mut tab.request.assertions;
            if let Some(existing) = form.row.and_then(|r| assertions.get_mut(r)) {
                existing.kind = kind;
            } else {
                assertions.push(Assertion::new(kind));
                app.editor.assertions_selected = assertions.len() - 1;
            }
            true
        }
        EditTarget::RawContentType => {
            if let Some(RequestBody::Raw { content_type, .. }) = &mut tab.request.body {
                let value = values.first().map_or("", |v| v.trim());
                *content_type = if value.is_empty() {
                    "text/plain".into()
                } else {
                    value.to_string()
                };
            }
            true
        }
        EditTarget::EnvVar
        | EditTarget::CollectionVar
        | EditTarget::NewEnvironment
        | EditTarget::RenameEnvironment => false,
    };
    if changed {
        tab.dirty = true;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Environment / collection variables modal
// ---------------------------------------------------------------------------

/// Open the variables modal (`E`, `:envedit`, `:colvars`).
pub fn open_env_editor(app: &mut App, scope: VarScope) {
    if scope == VarScope::Collection && active_collection_index(app).is_none() {
        app.notify(
            "No collection for this tab. Save it to a collection first".into(),
            NotificationKind::Warning,
        );
        return;
    }
    app.editor.env_scope = scope;
    app.editor.env_selected = 0;
    app.editor.form = None;
    app.editor.body = None;
    app.mode = AppMode::Modal(ModalKind::EnvironmentEdit);
}

fn active_collection_index(app: &App) -> Option<usize> {
    app.active_tab()
        .collection_index
        .filter(|ci| *ci < app.collections.len())
}

fn active_collection_mut(app: &mut App) -> Option<&mut hitt_core::core::collection::Collection> {
    let ci = active_collection_index(app)?;
    app.collections.get_mut(ci)
}

/// True if the active environment came from a `.env` file (never saved).
pub fn active_env_is_session_only(app: &App) -> bool {
    app.active_environment()
        .is_some_and(|e| app.session_env_ids.contains(&e.id))
}

fn persist_vars(app: &App, scope: VarScope) {
    match scope {
        VarScope::Environment => {
            if let Some(idx) = app.active_env {
                if !active_env_is_session_only(app) {
                    save_environment(app, idx);
                }
            }
        }
        VarScope::Collection => save_all_collections(app),
    }
}

pub(super) fn handle_env_editor_modal(app: &mut App, key: KeyEvent) {
    if app.editor.env_form().is_some() {
        form_key(app, key);
        return;
    }
    let scope = app.editor.env_scope;
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Char('q')) => {
            app.mode = AppMode::Normal;
            return;
        }
        (KeyModifiers::NONE, KeyCode::Char('c')) => {
            let next = match scope {
                VarScope::Environment => VarScope::Collection,
                VarScope::Collection => VarScope::Environment,
            };
            open_env_editor(app, next);
            return;
        }
        (KeyModifiers::NONE, KeyCode::Char('e')) if scope == VarScope::Environment => {
            cycle_environment(app);
            app.editor.env_selected = 0;
            return;
        }
        (KeyModifiers::NONE, KeyCode::Char('n')) => {
            let tab_id = app.active_tab().id;
            open_form(
                app,
                FieldForm::new(
                    EditTarget::NewEnvironment,
                    tab_id,
                    vec![FormField::text("Name", "")],
                ),
            );
            return;
        }
        (KeyModifiers::NONE, KeyCode::Char('v')) => {
            app.editor.show_secrets = !app.editor.show_secrets;
            return;
        }
        (_, KeyCode::Char('R')) if scope == VarScope::Environment => {
            if let Some(name) = app.active_environment().map(|e| e.name.clone()) {
                let tab_id = app.active_tab().id;
                open_form(
                    app,
                    FieldForm::new(
                        EditTarget::RenameEnvironment,
                        tab_id,
                        vec![FormField::text("Name", name)],
                    ),
                );
            } else {
                app.notify(
                    "No active environment to rename".into(),
                    NotificationKind::Warning,
                );
            }
            return;
        }
        (_, KeyCode::Char('D')) if scope == VarScope::Environment => {
            if let Some(idx) = app.active_env {
                request_delete_environment(app, idx);
            } else {
                app.notify(
                    "No active environment to delete".into(),
                    NotificationKind::Warning,
                );
            }
            return;
        }
        _ => {}
    }

    let tab_id = app.active_tab().id;
    match scope {
        VarScope::Environment => {
            let Some(env) = app.active_env.and_then(|i| app.environments.get_mut(i)) else {
                if matches!(key.code, KeyCode::Char('a') | KeyCode::Enter) {
                    app.notify(
                        "No active environment: press n to create one".into(),
                        NotificationKind::Warning,
                    );
                }
                return;
            };
            if key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Char('s') {
                let sel = app.editor.env_selected;
                if let Some(var) = env.values.get_mut(sel) {
                    var.secret = !var.secret;
                    persist_vars(app, scope);
                }
                return;
            }
            let rows = &mut env.values;
            match table_key(
                rows,
                &mut app.editor.env_selected,
                key,
                KvRow::toggle_enabled,
            ) {
                TableOutcome::Ignored | TableOutcome::Handled => {}
                TableOutcome::Changed => persist_vars(app, scope),
                TableOutcome::Add => {
                    rows.push(empty_env_var());
                    let row = rows.len() - 1;
                    app.editor.env_selected = row;
                    let mut form = FieldForm::key_value(EditTarget::EnvVar, tab_id, row, "", "");
                    form.is_new = true;
                    open_form(app, form);
                }
                TableOutcome::Edit(row) => {
                    let var = &rows[row];
                    let form =
                        FieldForm::key_value(EditTarget::EnvVar, tab_id, row, &var.key, &var.value);
                    open_form(app, form);
                }
            }
        }
        VarScope::Collection => {
            let Some(ci) = active_collection_index(app) else {
                return;
            };
            let rows = &mut app.collections[ci].variables;
            match table_key(
                rows,
                &mut app.editor.env_selected,
                key,
                KvRow::toggle_enabled,
            ) {
                TableOutcome::Ignored | TableOutcome::Handled => {}
                TableOutcome::Changed => persist_vars(app, scope),
                TableOutcome::Add => {
                    rows.push(KeyValuePair::new("", ""));
                    let row = rows.len() - 1;
                    app.editor.env_selected = row;
                    let mut form =
                        FieldForm::key_value(EditTarget::CollectionVar, tab_id, row, "", "");
                    form.is_new = true;
                    open_form(app, form);
                }
                TableOutcome::Edit(row) => {
                    let var = &rows[row];
                    let form = FieldForm::key_value(
                        EditTarget::CollectionVar,
                        tab_id,
                        row,
                        &var.key,
                        &var.value,
                    );
                    open_form(app, form);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// URL bar
// ---------------------------------------------------------------------------

/// Apply `edit` to the active tab's URL at the URL cursor (see
/// `EditorState::url_cursor`). The tab is marked dirty if the URL changed.
pub(super) fn edit_url(app: &mut App, edit: impl FnOnce(&mut TextInput)) {
    let idx = app.active_tab;
    let url = &mut app.tabs[idx].request.url;
    let cursor = app.editor.url_cursor.unwrap_or(usize::MAX);
    let mut input = TextInput::with_cursor(std::mem::take(url), cursor);
    let before = input.value.clone();
    edit(&mut input);
    app.editor.url_cursor = Some(input.cursor());
    let changed = input.value != before;
    *url = input.value;
    if changed {
        app.tabs[idx].dirty = true;
    }
}

// ---------------------------------------------------------------------------
// Bracketed paste
// ---------------------------------------------------------------------------

/// Insert pasted text into whatever input currently has the keyboard.
pub(super) fn handle_paste(app: &mut App, text: &str) {
    // Some terminals send bare `\r` line breaks.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    match &app.mode {
        AppMode::Insert => {
            if let Some(edit) = app.editor.body.as_mut() {
                edit.textarea.insert_str(&text);
            } else if app.editor.request_form().is_some() {
                paste_into_form(app, &text);
            } else if app.focus == crate::app::FocusArea::UrlBar {
                let line = text.trim().replace('\n', "");
                edit_url(app, |input| input.insert_str(&line));
            } else if app.focus == crate::app::FocusArea::ResponseBody {
                let line = text.trim_end_matches('\n').replace('\n', " ");
                app.active_tab_mut().ws_message_input.push_str(&line);
            }
        }
        AppMode::Modal(ModalKind::EnvironmentEdit) if app.editor.env_form().is_some() => {
            paste_into_form(app, &text);
        }
        AppMode::Command => {
            app.command_input
                .push_str(text.trim_end_matches('\n').lines().next().unwrap_or(""));
        }
        _ => {}
    }
}

fn paste_into_form(app: &mut App, text: &str) {
    if let Some(field) = app
        .editor
        .form
        .as_mut()
        .and_then(FieldForm::focused_field_mut)
    {
        if !field.is_choice() {
            field.input.insert_str(text);
        }
    }
}
