use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, SidebarSection};
use crate::core::collection::CollectionItem;
use crate::core::request::{HttpMethod, Protocol};
use crate::ui::theme::Theme;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Renders the active sidebar section content into `area`.
///
/// This function is called by `layout::render_sidebar_panel` after the section
/// tabs have already been drawn.  The area passed in is the remaining space
/// below those tabs.
pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    match app.sidebar_state.section {
        SidebarSection::Collections => render_collections(app, frame, area),
        SidebarSection::Chains => render_chains(app, frame, area),
        SidebarSection::History => render_history(app, frame, area),
    }
}

// ---------------------------------------------------------------------------
// Collections tree
// ---------------------------------------------------------------------------

fn render_collections(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    if app.collections.is_empty() {
        let empty = Paragraph::new("No collections loaded.\nPress 'i' to import.")
            .style(theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    // Build a flat list of visible items, respecting expand/collapse state.
    let mut items: Vec<SidebarRow> = Vec::new();
    for collection in &app.collections {
        let expanded = app.sidebar_state.expanded.contains(&collection.id);
        let chevron = if expanded { "v " } else { "> " };

        items.push(SidebarRow {
            kind: RowKind::CollectionHeader,
            depth: 0,
            label: format!("{}{}", chevron, collection.name),
            method: None,
            protocol: None,
            request_count: Some(collection.request_count()),
        });

        if expanded {
            flatten_items(
                &collection.items,
                1,
                &app.sidebar_state.expanded,
                &mut items,
            );
        }
    }

    // Apply scroll offset.
    let visible_height = area.height as usize;
    let scroll = app
        .sidebar_state
        .scroll_offset
        .min(items.len().saturating_sub(1));
    let visible_items = &items[scroll..items.len().min(scroll + visible_height)];

    let list_items: Vec<ListItem> = visible_items
        .iter()
        .enumerate()
        .map(|(view_idx, row)| {
            let absolute_idx = scroll + view_idx;
            let is_selected = absolute_idx == app.sidebar_state.selected;
            row_to_list_item(row, is_selected, theme)
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, area);
}

/// Recursively flattens collection items into `SidebarRow` entries, tracking
/// depth for indentation and honouring the expanded set.
fn flatten_items(
    items: &[CollectionItem],
    depth: u16,
    expanded: &std::collections::HashSet<uuid::Uuid>,
    out: &mut Vec<SidebarRow>,
) {
    for item in items {
        match item {
            CollectionItem::Request(req) => {
                out.push(SidebarRow {
                    kind: RowKind::Request,
                    depth,
                    label: req.name.clone(),
                    method: Some(req.method),
                    protocol: Some(req.protocol.clone()),
                    request_count: None,
                });
            }
            CollectionItem::Folder {
                id,
                name,
                items: children,
                ..
            } => {
                let is_expanded = expanded.contains(id);
                let chevron = if is_expanded { "v " } else { "> " };

                out.push(SidebarRow {
                    kind: RowKind::Folder,
                    depth,
                    label: format!("{chevron}{name}"),
                    method: None,
                    protocol: None,
                    request_count: None,
                });

                if is_expanded {
                    flatten_items(children, depth + 1, expanded, out);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Chains section
// ---------------------------------------------------------------------------

fn render_chains(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // Collect chains from all collections.
    let chains: Vec<(&str, &str)> = app
        .collections
        .iter()
        .flat_map(|c| {
            c.chains
                .iter()
                .map(move |ch| (c.name.as_str(), ch.name.as_str()))
        })
        .collect();

    if chains.is_empty() {
        let empty = Paragraph::new("No request chains defined.")
            .style(theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let scroll = app
        .sidebar_state
        .scroll_offset
        .min(chains.len().saturating_sub(1));
    let visible_height = area.height as usize;
    let visible = &chains[scroll..chains.len().min(scroll + visible_height)];

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(view_idx, (collection_name, chain_name))| {
            let absolute_idx = scroll + view_idx;
            let is_selected = absolute_idx == app.sidebar_state.selected;

            let chain_icon = Span::styled(
                ">> ",
                Style::default()
                    .fg(theme.colors.warning)
                    .add_modifier(Modifier::BOLD),
            );
            let name_span = Span::styled(*chain_name, Style::default().fg(theme.colors.foreground));
            let coll_span = Span::styled(format!("  ({collection_name})"), theme.muted_style());

            let line = Line::from(vec![chain_icon, name_span, coll_span]);

            if is_selected {
                ListItem::new(line).style(theme.selected_style())
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, area);
}

// ---------------------------------------------------------------------------
// History section
// ---------------------------------------------------------------------------

fn render_history(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let entries = app.history.entries();

    if entries.is_empty() {
        let empty = Paragraph::new("No request history yet.")
            .style(theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let scroll = app
        .sidebar_state
        .scroll_offset
        .min(entries.len().saturating_sub(1));
    let visible_height = area.height as usize;
    let visible = &entries[scroll..entries.len().min(scroll + visible_height)];

    // Available width for the URL portion (after method badge + padding).
    let url_max = (area.width as usize).saturating_sub(10);

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(view_idx, entry)| {
            let absolute_idx = scroll + view_idx;
            let is_selected = absolute_idx == app.sidebar_state.selected;

            let method_style = theme.method_style(&entry.method);
            let method_span = Span::styled(format!("{:<7}", entry.method.as_str()), method_style);

            let url_display = entry.short_url(url_max);
            let url_span = Span::styled(url_display, Style::default().fg(theme.colors.foreground));

            // Show status code if available.
            let status_span = if let Some(status) = entry.status {
                Span::styled(format!(" {status}"), theme.status_style(status))
            } else {
                Span::raw("")
            };

            let line = Line::from(vec![
                Span::raw(" "),
                method_span,
                Span::raw(" "),
                url_span,
                status_span,
            ]);

            if is_selected {
                ListItem::new(line).style(theme.selected_style())
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, area);
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum RowKind {
    CollectionHeader,
    Folder,
    Request,
}

#[derive(Debug)]
struct SidebarRow {
    kind: RowKind,
    depth: u16,
    label: String,
    method: Option<HttpMethod>,
    protocol: Option<Protocol>,
    request_count: Option<usize>,
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

/// Converts a `SidebarRow` into a styled `ListItem`.
fn row_to_list_item<'a>(row: &SidebarRow, selected: bool, theme: &Theme) -> ListItem<'a> {
    let indent = " ".repeat(row.depth as usize * 2);

    let spans: Vec<Span> = match row.kind {
        RowKind::CollectionHeader => {
            let mut s = vec![
                Span::raw(indent),
                Span::styled(
                    row.label.clone(),
                    Style::default()
                        .fg(theme.colors.foreground)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if let Some(count) = row.request_count {
                s.push(Span::styled(format!(" ({count})"), theme.muted_style()));
            }
            s
        }
        RowKind::Folder => {
            vec![
                Span::raw(indent),
                Span::styled(
                    row.label.clone(),
                    Style::default()
                        .fg(theme.colors.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]
        }
        RowKind::Request => {
            let badge = match &row.protocol {
                Some(Protocol::WebSocket) => Span::styled(
                    "WS     ",
                    Style::default()
                        .fg(theme.colors.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Some(Protocol::Sse) => Span::styled(
                    "SSE    ",
                    Style::default()
                        .fg(theme.colors.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Some(Protocol::Grpc { .. }) => Span::styled(
                    "gRPC   ",
                    Style::default()
                        .fg(theme.colors.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                _ => {
                    let method = row.method.unwrap_or(HttpMethod::GET);
                    method_badge(method, theme)
                }
            };
            vec![
                Span::raw(indent),
                badge,
                Span::raw(" "),
                Span::styled(
                    row.label.clone(),
                    Style::default().fg(theme.colors.foreground),
                ),
            ]
        }
    };

    let line = Line::from(spans);

    if selected {
        ListItem::new(line).style(theme.selected_style())
    } else {
        ListItem::new(line)
    }
}

/// Returns a short, coloured method badge such as `GET` or `POST`.
fn method_badge<'a>(method: HttpMethod, theme: &Theme) -> Span<'a> {
    let label = match method {
        HttpMethod::GET => "GET    ",
        HttpMethod::POST => "POST   ",
        HttpMethod::PUT => "PUT    ",
        HttpMethod::PATCH => "PATCH  ",
        HttpMethod::DELETE => "DEL    ",
        HttpMethod::HEAD => "HEAD   ",
        HttpMethod::OPTIONS => "OPT    ",
        HttpMethod::TRACE => "TRACE  ",
    };
    Span::styled(label, theme.method_style(&method))
}
