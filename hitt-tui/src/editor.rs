//! State and pure logic for the request / environment editors.
//!
//! Key handling lives in `event/editors.rs` and rendering in
//! `ui/widgets/request_editors.rs`; this module only knows how to turn
//! requests into editable fields and back.

use std::time::Duration;

use tui_textarea::TextArea;
use uuid::Uuid;

use hitt_core::core::auth::{ApiKeyLocation, AuthConfig, OAuth2GrantType};
use hitt_core::core::environment::EnvironmentVariable;
use hitt_core::core::request::{KeyValuePair, Request, RequestBody};
use hitt_core::testing::assertion_engine::{AssertionKind, JsonType};

// ---------------------------------------------------------------------------
// Single-line text input
// ---------------------------------------------------------------------------

/// A single-line text field with a char-indexed cursor.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    pub value: String,
    cursor: usize,
}

impl TextInput {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self { value, cursor }
    }

    /// Input with the cursor at char index `cursor` (clamped to the value).
    pub fn with_cursor(value: impl Into<String>, cursor: usize) -> Self {
        let value = value.into();
        let cursor = cursor.min(value.chars().count());
        Self { value, cursor }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn byte_idx(&self, char_idx: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_idx)
            .map_or(self.value.len(), |(i, _)| i)
    }

    pub fn insert_char(&mut self, c: char) {
        let idx = self.byte_idx(self.cursor);
        self.value.insert(idx, c);
        self.cursor += 1;
    }

    /// Insert pasted text; line breaks become spaces (the field is single-line).
    pub fn insert_str(&mut self, s: &str) {
        let cleaned: String = s
            .trim_end_matches(['\r', '\n'])
            .chars()
            .filter(|c| *c != '\r')
            .map(|c| if c == '\n' { ' ' } else { c })
            .collect();
        let idx = self.byte_idx(self.cursor);
        self.value.insert_str(idx, &cleaned);
        self.cursor += cleaned.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let idx = self.byte_idx(self.cursor);
            self.value.remove(idx);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.value.chars().count() {
            let idx = self.byte_idx(self.cursor);
            self.value.remove(idx);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.chars().count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    /// Delete everything before the cursor (Ctrl+U).
    pub fn clear_to_start(&mut self) {
        let idx = self.byte_idx(self.cursor);
        self.value.drain(..idx);
        self.cursor = 0;
    }
}

// ---------------------------------------------------------------------------
// Field forms (key/value rows, auth fields, assertions, env vars)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum FieldKind {
    Text,
    /// Fixed set of options cycled with Left/Right/Space.
    Choice {
        options: &'static [&'static str],
        selected: usize,
    },
}

#[derive(Debug, Clone)]
pub struct FormField {
    pub label: &'static str,
    pub input: TextInput,
    pub kind: FieldKind,
}

impl FormField {
    pub fn text(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            input: TextInput::new(value),
            kind: FieldKind::Text,
        }
    }

    pub fn choice(label: &'static str, options: &'static [&'static str], selected: usize) -> Self {
        Self {
            label,
            input: TextInput::default(),
            kind: FieldKind::Choice {
                options,
                selected: selected.min(options.len().saturating_sub(1)),
            },
        }
    }

    /// Current value: the typed text, or the selected option.
    pub fn value(&self) -> String {
        match &self.kind {
            FieldKind::Text => self.input.value.clone(),
            FieldKind::Choice { options, selected } => options[*selected].to_string(),
        }
    }

    pub fn is_choice(&self) -> bool {
        matches!(self.kind, FieldKind::Choice { .. })
    }

    /// Step a choice field forward / backward (no-op for text fields).
    pub fn cycle(&mut self, forward: bool) {
        if let FieldKind::Choice { options, selected } = &mut self.kind {
            let n = options.len();
            *selected = if forward {
                (*selected + 1) % n
            } else {
                (*selected + n - 1) % n
            };
        }
    }
}

/// What a `FieldForm` writes to when committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTarget {
    Param,
    Header,
    /// A row of a form-urlencoded / form-data body.
    FormBody,
    Auth,
    Assertion,
    RawContentType,
    EnvVar,
    CollectionVar,
    NewEnvironment,
    /// Rename the active environment (`R` in the environment modal).
    RenameEnvironment,
}

impl EditTarget {
    /// Targets edited inside the environment modal (vs. the request panel).
    pub fn is_env_modal(self) -> bool {
        matches!(
            self,
            Self::EnvVar | Self::CollectionVar | Self::NewEnvironment | Self::RenameEnvironment
        )
    }
}

/// An open edit form. While one is open every printable key goes to it.
#[derive(Debug, Clone)]
pub struct FieldForm {
    pub target: EditTarget,
    /// Row being edited (`None` for single-object targets or a new assertion).
    pub row: Option<usize>,
    /// The row was just added; cancelling removes it again.
    pub is_new: bool,
    /// Tab the form belongs to, so a tab switch can't redirect the commit.
    pub tab_id: Uuid,
    pub fields: Vec<FormField>,
    pub focused: usize,
    /// Validation error from the last commit attempt.
    pub error: Option<String>,
}

impl FieldForm {
    pub fn new(target: EditTarget, tab_id: Uuid, fields: Vec<FormField>) -> Self {
        Self {
            target,
            row: None,
            is_new: false,
            tab_id,
            fields,
            focused: 0,
            error: None,
        }
    }

    pub fn key_value(target: EditTarget, tab_id: Uuid, row: usize, key: &str, value: &str) -> Self {
        let mut form = Self::new(
            target,
            tab_id,
            vec![FormField::text("Key", key), FormField::text("Value", value)],
        );
        form.row = Some(row);
        form
    }

    pub fn values(&self) -> Vec<String> {
        self.fields.iter().map(FormField::value).collect()
    }

    pub fn focused_field_mut(&mut self) -> Option<&mut FormField> {
        self.fields.get_mut(self.focused)
    }

    pub fn focus_next(&mut self) {
        if !self.fields.is_empty() {
            self.focused = (self.focused + 1) % self.fields.len();
        }
    }

    pub fn focus_prev(&mut self) {
        if !self.fields.is_empty() {
            self.focused = (self.focused + self.fields.len() - 1) % self.fields.len();
        }
    }
}

// ---------------------------------------------------------------------------
// Multi-line body editing
// ---------------------------------------------------------------------------

/// Which text of the body the textarea edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyTextTarget {
    /// JSON / raw content, or the GraphQL query.
    Content,
    GraphqlVariables,
}

pub struct BodyEdit {
    pub tab_id: Uuid,
    pub target: BodyTextTarget,
    pub textarea: TextArea<'static>,
}

impl BodyEdit {
    pub fn new(tab_id: Uuid, target: BodyTextTarget, text: &str) -> Self {
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.split('\n').map(str::to_string).collect()
        };
        let mut textarea = TextArea::new(lines);
        textarea.set_tab_length(2);
        textarea.set_hard_tab_indent(false);
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        textarea.move_cursor(tui_textarea::CursorMove::Bottom);
        textarea.move_cursor(tui_textarea::CursorMove::End);
        Self {
            tab_id,
            target,
            textarea,
        }
    }

    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }
}

/// Which variable set the environment modal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VarScope {
    #[default]
    Environment,
    /// Variables of the active tab's collection.
    Collection,
}

/// Selection and in-progress edits for all editors.
#[derive(Default)]
pub struct EditorState {
    pub params_selected: usize,
    pub headers_selected: usize,
    pub form_body_selected: usize,
    pub auth_selected: usize,
    pub assertions_selected: usize,
    pub env_selected: usize,
    pub env_scope: VarScope,
    /// Reveal secrets (auth fields, secret env vars) instead of masking them.
    pub show_secrets: bool,
    pub form: Option<FieldForm>,
    pub body: Option<BodyEdit>,
    /// URL bar cursor (char index) while typing; `None` = end of the URL.
    pub url_cursor: Option<usize>,
    /// First URL char shown in the URL bar (horizontal scroll while typing).
    pub url_scroll: usize,
    /// Per-tab body content set aside while `t` cycles through other body
    /// types, so cycling back restores it.
    pub body_stash: std::collections::HashMap<Uuid, BodyStash>,
    /// First visible row of the request-panel table (params, headers, form
    /// body, assertions). Kept between frames so clicking a visible row
    /// doesn't re-scroll the table; updated while rendering.
    pub table_scroll: std::cell::Cell<usize>,
    /// Same, for the variables table in the environment modal.
    pub env_table_scroll: std::cell::Cell<usize>,
}

impl EditorState {
    /// The open form, if it edits something in the request panel.
    pub fn request_form(&self) -> Option<&FieldForm> {
        self.form.as_ref().filter(|f| !f.target.is_env_modal())
    }

    /// The open form, if it belongs to the environment modal.
    pub fn env_form(&self) -> Option<&FieldForm> {
        self.form.as_ref().filter(|f| f.target.is_env_modal())
    }
}

// ---------------------------------------------------------------------------
// Key/value rows
// ---------------------------------------------------------------------------

/// Rows shown by the key/value table editor.
pub trait KvRow {
    fn row_key(&self) -> &str;
    fn row_value(&self) -> &str;
    fn row_enabled(&self) -> bool;
    fn toggle_enabled(&mut self);
    fn set_key_value(&mut self, key: String, value: String);
    fn is_secret(&self) -> bool {
        false
    }
}

impl KvRow for KeyValuePair {
    fn row_key(&self) -> &str {
        &self.key
    }
    fn row_value(&self) -> &str {
        &self.value
    }
    fn row_enabled(&self) -> bool {
        self.enabled
    }
    fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
    }
    fn set_key_value(&mut self, key: String, value: String) {
        self.key = key;
        self.value = value;
    }
}

impl KvRow for EnvironmentVariable {
    fn row_key(&self) -> &str {
        &self.key
    }
    fn row_value(&self) -> &str {
        &self.value
    }
    fn row_enabled(&self) -> bool {
        self.enabled
    }
    fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
    }
    fn set_key_value(&mut self, key: String, value: String) {
        self.key = key;
        self.initial_value.clone_from(&value);
        self.value = value;
    }
    fn is_secret(&self) -> bool {
        self.secret
    }
}

pub fn empty_env_var() -> EnvironmentVariable {
    EnvironmentVariable {
        key: String::new(),
        value: String::new(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }
}

/// The key/value rows of a request edited by `target`, if it has any.
pub fn request_kv_rows_mut(
    request: &mut Request,
    target: EditTarget,
) -> Option<&mut Vec<KeyValuePair>> {
    match target {
        EditTarget::Param => Some(&mut request.params),
        EditTarget::Header => Some(&mut request.headers),
        EditTarget::FormBody => match &mut request.body {
            Some(RequestBody::FormUrlEncoded(pairs) | RequestBody::FormData(pairs)) => Some(pairs),
            _ => None,
        },
        _ => None,
    }
}

/// Secrets shorter than this are masked completely: showing the last 4
/// characters of a short secret would reveal most of it.
const MASK_REVEAL_MIN_LEN: usize = 12;

/// Mask a secret, keeping the last 4 characters of long ones visible.
pub fn mask_secret(value: &str) -> String {
    let len = value.chars().count();
    if len < MASK_REVEAL_MIN_LEN {
        "*".repeat(len.min(8))
    } else {
        let tail: String = value.chars().skip(len - 4).collect();
        format!("{}{tail}", "*".repeat((len - 4).min(8)))
    }
}

// ---------------------------------------------------------------------------
// Body types
// ---------------------------------------------------------------------------

/// Body types offered by `t`, in cycle order.
pub const BODY_TYPES: &[&str] = &[
    "none",
    "json",
    "raw",
    "form-urlencoded",
    "form-data",
    "graphql",
];

pub fn body_type_label(body: Option<&RequestBody>) -> &'static str {
    match body {
        None | Some(RequestBody::None) => "none",
        Some(RequestBody::Json(_)) => "json",
        Some(RequestBody::Raw { .. }) => "raw",
        Some(RequestBody::FormUrlEncoded(_)) => "form-urlencoded",
        Some(RequestBody::FormData(_)) => "form-data",
        Some(RequestBody::GraphQL { .. }) => "graphql",
        Some(RequestBody::Binary(_)) => "binary",
        Some(RequestBody::Protobuf { .. }) => "protobuf",
    }
}

/// Body content set aside while cycling body types (`t`): the text of the
/// last text-like body, the raw content type, GraphQL variables and the rows
/// of the last form body.
#[derive(Debug, Clone, Default)]
pub struct BodyStash {
    pub text: Option<String>,
    pub content_type: Option<String>,
    pub graphql_variables: Option<String>,
    pub form_rows: Option<Vec<KeyValuePair>>,
}

impl BodyStash {
    /// Remember the content of `body` so a later type change can restore it.
    fn remember(&mut self, body: Option<&RequestBody>) {
        match body {
            Some(RequestBody::Json(s)) => self.text = Some(s.clone()),
            Some(RequestBody::Raw {
                content,
                content_type,
            }) => {
                self.text = Some(content.clone());
                self.content_type = Some(content_type.clone());
            }
            Some(RequestBody::GraphQL { query, variables }) => {
                self.text = Some(query.clone());
                self.graphql_variables.clone_from(variables);
            }
            Some(RequestBody::FormUrlEncoded(p) | RequestBody::FormData(p)) => {
                self.form_rows = Some(p.clone());
            }
            _ => {}
        }
    }
}

/// Body type after `body`. Text carries over between text-like types and rows
/// between form types; content left behind (text while on a form type, rows
/// while on a text type, a custom raw content type) is kept in `stash` and
/// restored when cycling back. Binary / protobuf bodies restart at none.
pub fn next_body_type(body: Option<&RequestBody>, stash: &mut BodyStash) -> Option<RequestBody> {
    let current = BODY_TYPES
        .iter()
        .position(|t| *t == body_type_label(body))
        .unwrap_or(BODY_TYPES.len() - 1);
    let next = BODY_TYPES[(current + 1) % BODY_TYPES.len()];

    stash.remember(body);
    let text = stash.text.clone().unwrap_or_default();
    let pairs = stash.form_rows.clone().unwrap_or_default();
    match next {
        "json" => Some(RequestBody::Json(text)),
        "raw" => Some(RequestBody::Raw {
            content: text,
            content_type: stash
                .content_type
                .clone()
                .unwrap_or_else(|| "text/plain".into()),
        }),
        "form-urlencoded" => Some(RequestBody::FormUrlEncoded(pairs)),
        "form-data" => Some(RequestBody::FormData(pairs)),
        "graphql" => Some(RequestBody::GraphQL {
            query: text,
            variables: stash.graphql_variables.clone(),
        }),
        _ => None,
    }
}

/// True for bodies edited with the multi-line text editor.
pub fn is_text_body(body: Option<&RequestBody>) -> bool {
    matches!(
        body,
        Some(RequestBody::Json(_) | RequestBody::Raw { .. } | RequestBody::GraphQL { .. })
    )
}

pub fn is_form_body(body: Option<&RequestBody>) -> bool {
    matches!(
        body,
        Some(RequestBody::FormUrlEncoded(_) | RequestBody::FormData(_))
    )
}

pub fn body_text(body: Option<&RequestBody>, target: BodyTextTarget) -> Option<String> {
    match (body?, target) {
        (RequestBody::Json(s), BodyTextTarget::Content) => Some(s.clone()),
        (RequestBody::Raw { content, .. }, BodyTextTarget::Content) => Some(content.clone()),
        (RequestBody::GraphQL { query, .. }, BodyTextTarget::Content) => Some(query.clone()),
        (RequestBody::GraphQL { variables, .. }, BodyTextTarget::GraphqlVariables) => {
            Some(variables.clone().unwrap_or_default())
        }
        _ => None,
    }
}

/// Write edited text back. Returns true if the body changed.
pub fn set_body_text(body: Option<&mut RequestBody>, target: BodyTextTarget, text: String) -> bool {
    let slot = match (body, target) {
        (Some(RequestBody::Json(s)), BodyTextTarget::Content) => s,
        (Some(RequestBody::Raw { content, .. }), BodyTextTarget::Content) => content,
        (Some(RequestBody::GraphQL { query, .. }), BodyTextTarget::Content) => query,
        (Some(RequestBody::GraphQL { variables, .. }), BodyTextTarget::GraphqlVariables) => {
            let new = if text.trim().is_empty() {
                None
            } else {
                Some(text)
            };
            if *variables == new {
                return false;
            }
            *variables = new;
            return true;
        }
        _ => return false,
    };
    if *slot == text {
        false
    } else {
        *slot = text;
        true
    }
}

/// Parse error of a JSON body, if any (`{{var}}` placeholders may cause false alarms).
pub fn json_error(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(text)
        .err()
        .map(|e| e.to_string())
}

/// Pretty-print JSON with 2-space indentation, keeping the original key order
/// (serde_json's `Value` would sort keys). Errors if `text` isn't valid JSON.
pub fn format_json(text: &str) -> Result<String, String> {
    serde_json::from_str::<serde_json::Value>(text).map_err(|e| e.to_string())?;

    let mut out = String::with_capacity(text.len() * 2);
    let mut indent = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = text.chars().peekable();
    let newline = |out: &mut String, indent: usize| {
        out.push('\n');
        out.push_str(&"  ".repeat(indent));
    };
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '{' | '[' => {
                out.push(c);
                // Keep empty containers on one line.
                while chars.peek().is_some_and(|n| n.is_whitespace()) {
                    chars.next();
                }
                if matches!(chars.peek(), Some('}' | ']')) {
                    out.push(chars.next().unwrap_or(']'));
                } else {
                    indent += 1;
                    newline(&mut out, indent);
                }
            }
            '}' | ']' => {
                indent = indent.saturating_sub(1);
                newline(&mut out, indent);
                out.push(c);
            }
            ',' => {
                out.push(c);
                newline(&mut out, indent);
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/// Auth types offered by `t`, in cycle order. "inherit" (the default for a
/// request without auth) uses the folder / collection auth; "none" sends no
/// auth at all, even inside a collection that has one.
pub const AUTH_TYPES: &[&str] = &["inherit", "none", "bearer", "basic", "apikey", "oauth2"];
const API_KEY_LOCATIONS: &[&str] = &["header", "query"];
const GRANT_TYPES: &[&str] = &[
    "client_credentials",
    "authorization_code",
    "password",
    "implicit",
];

fn auth_type_index(auth: Option<&AuthConfig>) -> usize {
    match auth {
        None | Some(AuthConfig::Inherit) => 0,
        Some(AuthConfig::None) => 1,
        Some(AuthConfig::Bearer { .. }) => 2,
        Some(AuthConfig::Basic { .. }) => 3,
        Some(AuthConfig::ApiKey { .. }) => 4,
        Some(AuthConfig::OAuth2 { .. }) => 5,
    }
}

/// Display name of a request's auth; an unset auth inherits from the parent.
pub fn auth_type_name(auth: Option<&AuthConfig>) -> &'static str {
    auth.map_or_else(
        || AuthConfig::Inherit.display_name(),
        AuthConfig::display_name,
    )
}

/// True if the request uses the auth of its folder / collection.
pub fn auth_inherits(auth: Option<&AuthConfig>) -> bool {
    matches!(auth, None | Some(AuthConfig::Inherit))
}

/// Auth config after `auth` in `AUTH_TYPES` order, with empty fields.
/// Inherit is represented as no auth (`None`), like freshly imported requests.
pub fn next_auth_type(auth: Option<&AuthConfig>) -> Option<AuthConfig> {
    match AUTH_TYPES[(auth_type_index(auth) + 1) % AUTH_TYPES.len()] {
        "none" => Some(AuthConfig::None),
        "bearer" => Some(AuthConfig::bearer("")),
        "basic" => Some(AuthConfig::basic("", "")),
        "apikey" => Some(AuthConfig::ApiKey {
            key: "X-API-Key".into(),
            value: String::new(),
            location: ApiKeyLocation::Header,
        }),
        "oauth2" => Some(AuthConfig::OAuth2 {
            grant_type: OAuth2GrantType::ClientCredentials,
            access_token_url: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            scope: None,
            token: None,
        }),
        _ => None,
    }
}

/// One displayed / editable auth field.
pub struct AuthRow {
    pub label: &'static str,
    pub value: String,
    pub secret: bool,
    pub choices: Option<&'static [&'static str]>,
}

fn row(label: &'static str, value: &str, secret: bool) -> AuthRow {
    AuthRow {
        label,
        value: value.to_string(),
        secret,
        choices: None,
    }
}

pub fn auth_rows(auth: Option<&AuthConfig>) -> Vec<AuthRow> {
    match auth {
        Some(AuthConfig::Bearer { token }) => vec![row("Token", token, true)],
        Some(AuthConfig::Basic { username, password }) => {
            vec![
                row("Username", username, false),
                row("Password", password, true),
            ]
        }
        Some(AuthConfig::ApiKey {
            key,
            value,
            location,
        }) => vec![
            row("Key", key, false),
            row("Value", value, true),
            AuthRow {
                label: "Add to",
                value: match location {
                    ApiKeyLocation::Header => "header",
                    ApiKeyLocation::QueryParam => "query",
                }
                .into(),
                secret: false,
                choices: Some(API_KEY_LOCATIONS),
            },
        ],
        Some(AuthConfig::OAuth2 {
            grant_type,
            access_token_url,
            client_id,
            client_secret,
            scope,
            token,
        }) => vec![
            AuthRow {
                label: "Grant type",
                value: grant_type.as_str().into(),
                secret: false,
                choices: Some(GRANT_TYPES),
            },
            row("Token URL", access_token_url, false),
            row("Client ID", client_id, false),
            row("Client secret", client_secret, true),
            row("Scope", scope.as_deref().unwrap_or(""), false),
            row("Token", token.as_deref().unwrap_or(""), true),
        ],
        Some(AuthConfig::Inherit | AuthConfig::None) | None => Vec::new(),
    }
}

pub fn auth_form_fields(auth: Option<&AuthConfig>) -> Vec<FormField> {
    auth_rows(auth)
        .into_iter()
        .map(|r| match r.choices {
            Some(options) => FormField::choice(
                r.label,
                options,
                options.iter().position(|o| *o == r.value).unwrap_or(0),
            ),
            None => FormField::text(r.label, r.value),
        })
        .collect()
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Write form values (in `auth_rows` order) back into `auth`.
pub fn apply_auth_values(auth: &mut AuthConfig, values: &[String]) {
    let v = |i: usize| values.get(i).cloned().unwrap_or_default();
    match auth {
        AuthConfig::Bearer { token } => *token = v(0),
        AuthConfig::Basic { username, password } => {
            *username = v(0);
            *password = v(1);
        }
        AuthConfig::ApiKey {
            key,
            value,
            location,
        } => {
            *key = v(0);
            *value = v(1);
            *location = if v(2) == "query" {
                ApiKeyLocation::QueryParam
            } else {
                ApiKeyLocation::Header
            };
        }
        AuthConfig::OAuth2 {
            grant_type,
            access_token_url,
            client_id,
            client_secret,
            scope,
            token,
        } => {
            *grant_type = match v(0).as_str() {
                "authorization_code" => OAuth2GrantType::AuthorizationCode,
                "password" => OAuth2GrantType::PasswordCredentials,
                "implicit" => OAuth2GrantType::Implicit,
                _ => OAuth2GrantType::ClientCredentials,
            };
            *access_token_url = v(1);
            *client_id = v(2);
            *client_secret = v(3);
            *scope = non_empty(&v(4));
            *token = non_empty(&v(5));
        }
        AuthConfig::Inherit | AuthConfig::None => {}
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// Assertion kinds offered by the form, in `AssertionKind` order.
pub const ASSERTION_KINDS: &[&str] = &[
    "status equals",
    "status in range",
    "JSONPath exists",
    "JSONPath equals",
    "JSONPath type",
    "JSONPath contains",
    "body contains",
    "header exists",
    "header equals",
    "response time <",
    "size <",
    "JSON schema",
];
const JSON_TYPES: &[&str] = &["string", "number", "boolean", "array", "object", "null"];

fn assertion_kind_index(kind: &AssertionKind) -> usize {
    match kind {
        AssertionKind::StatusEquals(_) => 0,
        AssertionKind::StatusRange(..) => 1,
        AssertionKind::BodyPathExists(_) => 2,
        AssertionKind::BodyPathEquals { .. } => 3,
        AssertionKind::BodyPathType { .. } => 4,
        AssertionKind::BodyPathContains { .. } => 5,
        AssertionKind::BodyContains(_) => 6,
        AssertionKind::HeaderExists(_) => 7,
        AssertionKind::HeaderEquals { .. } => 8,
        AssertionKind::ResponseTimeLessThan(_) => 9,
        AssertionKind::SizeLessThan(_) => 10,
        AssertionKind::MatchesJsonSchema(_) => 11,
    }
}

fn json_type_name(t: &JsonType) -> &'static str {
    match t {
        JsonType::String => "string",
        JsonType::Number => "number",
        JsonType::Boolean => "boolean",
        JsonType::Array => "array",
        JsonType::Object => "object",
        JsonType::Null => "null",
    }
}

/// Field values of an existing assertion, in `assertion_fields` order.
fn assertion_values(kind: &AssertionKind) -> Vec<String> {
    match kind {
        AssertionKind::StatusEquals(s) => vec![s.to_string()],
        AssertionKind::StatusRange(min, max) => vec![min.to_string(), max.to_string()],
        AssertionKind::BodyPathExists(p)
        | AssertionKind::BodyContains(p)
        | AssertionKind::HeaderExists(p) => {
            vec![p.clone()]
        }
        AssertionKind::BodyPathEquals { path, expected } => {
            vec![path.clone(), expected.to_string()]
        }
        AssertionKind::BodyPathType { path, expected } => {
            vec![path.clone(), json_type_name(expected).into()]
        }
        AssertionKind::BodyPathContains { path, substring } => {
            vec![path.clone(), substring.clone()]
        }
        AssertionKind::HeaderEquals { name, expected } => vec![name.clone(), expected.clone()],
        AssertionKind::ResponseTimeLessThan(d) => vec![d.as_millis().to_string()],
        AssertionKind::SizeLessThan(s) => vec![s.to_string()],
        AssertionKind::MatchesJsonSchema(v) => vec![v.to_string()],
    }
}

/// Form fields for assertion kind `kind_idx`: the kind selector followed by
/// its parameters, pre-filled from `prev` by position.
pub fn assertion_fields(kind_idx: usize, prev: &[String]) -> Vec<FormField> {
    let p = |i: usize| prev.get(i).cloned().unwrap_or_default();
    let mut fields = vec![FormField::choice("Kind", ASSERTION_KINDS, kind_idx)];
    match kind_idx {
        0 => fields.push(FormField::text("Status", p(0))),
        1 => {
            fields.push(FormField::text("Min", p(0)));
            fields.push(FormField::text("Max", p(1)));
        }
        2 => fields.push(FormField::text("JSONPath", p(0))),
        3 => {
            fields.push(FormField::text("JSONPath", p(0)));
            fields.push(FormField::text("Expected (JSON)", p(1)));
        }
        4 => {
            fields.push(FormField::text("JSONPath", p(0)));
            let selected = JSON_TYPES.iter().position(|t| *t == p(1)).unwrap_or(0);
            fields.push(FormField::choice("Type", JSON_TYPES, selected));
        }
        5 => {
            fields.push(FormField::text("JSONPath", p(0)));
            fields.push(FormField::text("Substring", p(1)));
        }
        6 => fields.push(FormField::text("Text", p(0))),
        7 => fields.push(FormField::text("Header", p(0))),
        8 => {
            fields.push(FormField::text("Header", p(0)));
            fields.push(FormField::text("Expected", p(1)));
        }
        9 => fields.push(FormField::text("Max ms", p(0))),
        10 => fields.push(FormField::text("Max bytes", p(0))),
        _ => fields.push(FormField::text("Schema (JSON)", p(0))),
    }
    fields
}

/// Fields for assertion kind `kind_idx` after the kind selector changed:
/// only values of fields with the same meaning (same label, e.g. "JSONPath"
/// or "Header") carry over; everything else starts empty.
pub fn assertion_fields_for_kind(kind_idx: usize, old: &[FormField]) -> Vec<FormField> {
    let mut fields = assertion_fields(kind_idx, &[]);
    for field in fields.iter_mut().skip(1).filter(|f| !f.is_choice()) {
        if let Some(prev) = old
            .iter()
            .skip(1)
            .find(|o| o.label == field.label && !o.is_choice())
        {
            field.input = TextInput::new(prev.input.value.clone());
        }
    }
    fields
}

/// Fields for editing an existing assertion.
pub fn assertion_form_fields(kind: &AssertionKind) -> Vec<FormField> {
    assertion_fields(assertion_kind_index(kind), &assertion_values(kind))
}

/// Selected kind of an assertion form (its first field).
pub fn assertion_form_kind(form: &FieldForm) -> usize {
    match form.fields.first().map(|f| &f.kind) {
        Some(FieldKind::Choice { selected, .. }) => *selected,
        _ => 0,
    }
}

fn parse_num<T: std::str::FromStr>(label: &str, s: &str) -> Result<T, String> {
    s.trim()
        .parse()
        .map_err(|_| format!("{label}: '{}' is not a valid number", s.trim()))
}

fn required(label: &str, s: &str) -> Result<String, String> {
    if s.trim().is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(s.trim().to_string())
    }
}

/// Build an assertion from form values (kind selector first).
pub fn build_assertion_kind(values: &[String]) -> Result<AssertionKind, String> {
    let kind = ASSERTION_KINDS
        .iter()
        .position(|k| Some(*k) == values.first().map(String::as_str))
        .unwrap_or(0);
    let v = |i: usize| values.get(i + 1).cloned().unwrap_or_default();
    Ok(match kind {
        0 => AssertionKind::StatusEquals(parse_num("Status", &v(0))?),
        1 => {
            let min: u16 = parse_num("Min", &v(0))?;
            let max: u16 = parse_num("Max", &v(1))?;
            if min > max {
                return Err("Min must be <= Max".into());
            }
            AssertionKind::StatusRange(min, max)
        }
        2 => AssertionKind::BodyPathExists(required("JSONPath", &v(0))?),
        3 => {
            let raw = v(1);
            // Accept bare words as strings: `ok` == `"ok"`.
            let expected = serde_json::from_str(raw.trim())
                .unwrap_or_else(|_| serde_json::Value::String(raw.clone()));
            AssertionKind::BodyPathEquals {
                path: required("JSONPath", &v(0))?,
                expected,
            }
        }
        4 => AssertionKind::BodyPathType {
            path: required("JSONPath", &v(0))?,
            expected: match v(1).as_str() {
                "number" => JsonType::Number,
                "boolean" => JsonType::Boolean,
                "array" => JsonType::Array,
                "object" => JsonType::Object,
                "null" => JsonType::Null,
                _ => JsonType::String,
            },
        },
        5 => AssertionKind::BodyPathContains {
            path: required("JSONPath", &v(0))?,
            substring: v(1),
        },
        6 => AssertionKind::BodyContains(required("Text", &v(0))?),
        7 => AssertionKind::HeaderExists(required("Header", &v(0))?),
        8 => AssertionKind::HeaderEquals {
            name: required("Header", &v(0))?,
            expected: v(1),
        },
        9 => {
            AssertionKind::ResponseTimeLessThan(Duration::from_millis(parse_num("Max ms", &v(0))?))
        }
        10 => AssertionKind::SizeLessThan(parse_num("Max bytes", &v(0))?),
        _ => AssertionKind::MatchesJsonSchema(
            serde_json::from_str(v(0).trim()).map_err(|e| format!("Invalid JSON schema: {e}"))?,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_input_handles_multibyte_cursor() {
        let mut input = TextInput::new("añb");
        input.left();
        input.backspace();
        assert_eq!(input.value, "ab");
        input.insert_char('é');
        assert_eq!(input.value, "aéb");
        input.home();
        input.delete();
        assert_eq!(input.value, "éb");
    }

    #[test]
    fn mask_keeps_last_four_of_long_secrets_only() {
        assert_eq!(mask_secret("abcdefghijklmnop"), "********mnop");
        assert_eq!(mask_secret("abc123"), "******");
        assert_eq!(mask_secret("abcdefghijk"), "********");
        assert_eq!(mask_secret(""), "");
    }

    #[test]
    fn body_type_cycle_carries_text() {
        let mut stash = BodyStash::default();
        let json = RequestBody::Json("{}".into());
        let raw = next_body_type(Some(&json), &mut stash);
        assert!(matches!(raw, Some(RequestBody::Raw { ref content, .. }) if content == "{}"));
        let after_graphql = next_body_type(
            Some(&RequestBody::GraphQL {
                query: String::new(),
                variables: None,
            }),
            &mut stash,
        );
        assert!(after_graphql.is_none());
    }

    #[test]
    fn body_type_cycle_restores_stashed_content() {
        let mut stash = BodyStash::default();
        let mut body = Some(RequestBody::Raw {
            content: "<a/>".into(),
            content_type: "application/xml".into(),
        });
        // raw -> form-urlencoded -> form-data -> graphql -> none -> json -> raw
        let mut seen = Vec::new();
        for _ in 0..6 {
            body = next_body_type(body.as_ref(), &mut stash);
            if let Some(RequestBody::FormUrlEncoded(rows)) = &mut body {
                rows.push(KeyValuePair::new("k", "v"));
            }
            seen.push(body_type_label(body.as_ref()));
        }
        assert_eq!(
            seen,
            [
                "form-urlencoded",
                "form-data",
                "graphql",
                "none",
                "json",
                "raw"
            ]
        );
        assert!(matches!(
            &body,
            Some(RequestBody::Raw { content, content_type })
                if content == "<a/>" && content_type == "application/xml"
        ));
        // Cycling on to the form types brings the rows back.
        body = next_body_type(body.as_ref(), &mut stash);
        assert!(matches!(&body, Some(RequestBody::FormUrlEncoded(r)) if r.len() == 1));
    }

    #[test]
    fn auth_cycle_separates_inherit_from_no_auth() {
        let mut auth: Option<AuthConfig> = None;
        let mut names = vec![auth_type_name(auth.as_ref())];
        for _ in 0..6 {
            auth = next_auth_type(auth.as_ref());
            names.push(auth_type_name(auth.as_ref()));
        }
        assert_eq!(
            names,
            [
                "Inherit from parent",
                "No Auth",
                "Bearer Token",
                "Basic Auth",
                "API Key",
                "OAuth 2.0",
                "Inherit from parent",
            ]
        );
        assert!(matches!(
            next_auth_type(Some(&AuthConfig::Inherit)),
            Some(AuthConfig::None)
        ));
    }

    #[test]
    fn assertion_kind_change_keeps_only_matching_fields() {
        let status = assertion_fields(0, &["200".to_string()]);
        let body = assertion_fields_for_kind(6, &status);
        assert_eq!(body[1].label, "Text");
        assert_eq!(body[1].input.value, "");

        let mut path = assertion_fields(2, &["$.id".to_string()]);
        path[1].input = TextInput::new("$.id");
        let equals = assertion_fields_for_kind(3, &path);
        assert_eq!(equals[1].input.value, "$.id");
        assert_eq!(equals[2].input.value, "");
    }

    #[test]
    fn format_json_keeps_key_order() {
        let out = format_json(r#"{"b":1,"a":[],"c":{"s":"x, y: {"}}"#).unwrap();
        assert_eq!(
            out,
            "{\n  \"b\": 1,\n  \"a\": [],\n  \"c\": {\n    \"s\": \"x, y: {\"\n  }\n}"
        );
        assert!(format_json("{oops").is_err());
    }

    #[test]
    fn assertion_round_trips_through_form() {
        let kind = AssertionKind::BodyPathEquals {
            path: "$.id".into(),
            expected: serde_json::json!("42"),
        };
        let form = FieldForm::new(
            EditTarget::Assertion,
            Uuid::nil(),
            assertion_form_fields(&kind),
        );
        let rebuilt = build_assertion_kind(&form.values()).unwrap();
        assert_eq!(rebuilt.description(), kind.description());
    }
}
