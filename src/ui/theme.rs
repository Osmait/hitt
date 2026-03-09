use anyhow::Result;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

use crate::core::request::HttpMethod;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub colors: ThemeColors,
    pub border_style: BorderStyle,
    pub sidebar_width: u16,
}

#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub muted: Color,
    pub methods: MethodColors,
    pub status: StatusColors,
    pub syntax: SyntaxColors,
}

#[derive(Debug, Clone)]
pub struct MethodColors {
    pub get: Color,
    pub post: Color,
    pub put: Color,
    pub patch: Color,
    pub delete: Color,
    pub head: Color,
    pub options: Color,
    pub ws: Color,
    pub sse: Color,
}

#[derive(Debug, Clone)]
pub struct StatusColors {
    pub info: Color,
    pub success: Color,
    pub redirect: Color,
    pub client_error: Color,
    pub server_error: Color,
}

#[derive(Debug, Clone)]
pub struct SyntaxColors {
    pub json_key: Color,
    pub json_string: Color,
    pub json_number: Color,
    pub json_boolean: Color,
    pub json_null: Color,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderStyle {
    Rounded,
    Plain,
    Double,
    Thick,
}

impl Theme {
    pub fn load(name: &str) -> Result<Self> {
        match name {
            "catppuccin" => Ok(Self::catppuccin()),
            "dracula" => Ok(Self::dracula()),
            "gruvbox" => Ok(Self::gruvbox()),
            "tokyo-night" => Ok(Self::tokyo_night()),
            _ => anyhow::bail!("Unknown theme: {}", name),
        }
    }

    pub fn catppuccin() -> Self {
        Self {
            name: "catppuccin".to_string(),
            colors: ThemeColors {
                background: hex_color("#1E1E2E"),
                foreground: hex_color("#CDD6F4"),
                accent: hex_color("#89B4FA"),
                success: hex_color("#A6E3A1"),
                warning: hex_color("#F9E2AF"),
                error: hex_color("#F38BA8"),
                muted: hex_color("#6C7086"),
                methods: MethodColors {
                    get: hex_color("#A6E3A1"),
                    post: hex_color("#F9E2AF"),
                    put: hex_color("#89B4FA"),
                    patch: hex_color("#CBA6F7"),
                    delete: hex_color("#F38BA8"),
                    head: hex_color("#94E2D5"),
                    options: hex_color("#F5C2E7"),
                    ws: hex_color("#89B4FA"),
                    sse: hex_color("#A6E3A1"),
                },
                status: StatusColors {
                    info: hex_color("#89B4FA"),
                    success: hex_color("#A6E3A1"),
                    redirect: hex_color("#F9E2AF"),
                    client_error: hex_color("#FAB387"),
                    server_error: hex_color("#F38BA8"),
                },
                syntax: SyntaxColors {
                    json_key: hex_color("#89B4FA"),
                    json_string: hex_color("#A6E3A1"),
                    json_number: hex_color("#FAB387"),
                    json_boolean: hex_color("#CBA6F7"),
                    json_null: hex_color("#6C7086"),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    pub fn dracula() -> Self {
        Self {
            name: "dracula".to_string(),
            colors: ThemeColors {
                background: hex_color("#282A36"),
                foreground: hex_color("#F8F8F2"),
                accent: hex_color("#BD93F9"),
                success: hex_color("#50FA7B"),
                warning: hex_color("#F1FA8C"),
                error: hex_color("#FF5555"),
                muted: hex_color("#6272A4"),
                methods: MethodColors {
                    get: hex_color("#50FA7B"),
                    post: hex_color("#F1FA8C"),
                    put: hex_color("#8BE9FD"),
                    patch: hex_color("#BD93F9"),
                    delete: hex_color("#FF5555"),
                    head: hex_color("#8BE9FD"),
                    options: hex_color("#FF79C6"),
                    ws: hex_color("#BD93F9"),
                    sse: hex_color("#50FA7B"),
                },
                status: StatusColors {
                    info: hex_color("#8BE9FD"),
                    success: hex_color("#50FA7B"),
                    redirect: hex_color("#F1FA8C"),
                    client_error: hex_color("#FFB86C"),
                    server_error: hex_color("#FF5555"),
                },
                syntax: SyntaxColors {
                    json_key: hex_color("#8BE9FD"),
                    json_string: hex_color("#F1FA8C"),
                    json_number: hex_color("#BD93F9"),
                    json_boolean: hex_color("#FF79C6"),
                    json_null: hex_color("#6272A4"),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    pub fn gruvbox() -> Self {
        Self {
            name: "gruvbox".to_string(),
            colors: ThemeColors {
                background: hex_color("#282828"),
                foreground: hex_color("#EBDBB2"),
                accent: hex_color("#83A598"),
                success: hex_color("#B8BB26"),
                warning: hex_color("#FABD2F"),
                error: hex_color("#FB4934"),
                muted: hex_color("#928374"),
                methods: MethodColors {
                    get: hex_color("#B8BB26"),
                    post: hex_color("#FABD2F"),
                    put: hex_color("#83A598"),
                    patch: hex_color("#D3869B"),
                    delete: hex_color("#FB4934"),
                    head: hex_color("#8EC07C"),
                    options: hex_color("#D3869B"),
                    ws: hex_color("#83A598"),
                    sse: hex_color("#B8BB26"),
                },
                status: StatusColors {
                    info: hex_color("#83A598"),
                    success: hex_color("#B8BB26"),
                    redirect: hex_color("#FABD2F"),
                    client_error: hex_color("#FE8019"),
                    server_error: hex_color("#FB4934"),
                },
                syntax: SyntaxColors {
                    json_key: hex_color("#83A598"),
                    json_string: hex_color("#B8BB26"),
                    json_number: hex_color("#D3869B"),
                    json_boolean: hex_color("#FE8019"),
                    json_null: hex_color("#928374"),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            name: "tokyo-night".to_string(),
            colors: ThemeColors {
                background: hex_color("#1A1B26"),
                foreground: hex_color("#C0CAF5"),
                accent: hex_color("#7AA2F7"),
                success: hex_color("#9ECE6A"),
                warning: hex_color("#E0AF68"),
                error: hex_color("#F7768E"),
                muted: hex_color("#565F89"),
                methods: MethodColors {
                    get: hex_color("#9ECE6A"),
                    post: hex_color("#E0AF68"),
                    put: hex_color("#7AA2F7"),
                    patch: hex_color("#BB9AF7"),
                    delete: hex_color("#F7768E"),
                    head: hex_color("#73DACA"),
                    options: hex_color("#FF9E64"),
                    ws: hex_color("#7AA2F7"),
                    sse: hex_color("#9ECE6A"),
                },
                status: StatusColors {
                    info: hex_color("#7AA2F7"),
                    success: hex_color("#9ECE6A"),
                    redirect: hex_color("#E0AF68"),
                    client_error: hex_color("#FF9E64"),
                    server_error: hex_color("#F7768E"),
                },
                syntax: SyntaxColors {
                    json_key: hex_color("#7AA2F7"),
                    json_string: hex_color("#9ECE6A"),
                    json_number: hex_color("#FF9E64"),
                    json_boolean: hex_color("#BB9AF7"),
                    json_null: hex_color("#565F89"),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    // Style helpers
    pub fn method_style(&self, method: &HttpMethod) -> Style {
        let color = match method {
            HttpMethod::GET => self.colors.methods.get,
            HttpMethod::POST => self.colors.methods.post,
            HttpMethod::PUT => self.colors.methods.put,
            HttpMethod::PATCH => self.colors.methods.patch,
            HttpMethod::DELETE => self.colors.methods.delete,
            HttpMethod::HEAD => self.colors.methods.head,
            HttpMethod::OPTIONS | HttpMethod::TRACE => self.colors.methods.options,
        };
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }

    pub fn protocol_style_ws(&self) -> Style {
        Style::default().fg(self.colors.methods.ws).add_modifier(Modifier::BOLD)
    }

    pub fn protocol_style_sse(&self) -> Style {
        Style::default().fg(self.colors.methods.sse).add_modifier(Modifier::BOLD)
    }

    pub fn status_style(&self, status: u16) -> Style {
        let color = match status {
            100..=199 => self.colors.status.info,
            200..=299 => self.colors.status.success,
            300..=399 => self.colors.status.redirect,
            400..=499 => self.colors.status.client_error,
            500..=599 => self.colors.status.server_error,
            _ => self.colors.muted,
        };
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }

    pub fn focused_border_style(&self) -> Style {
        Style::default().fg(self.colors.accent)
    }

    /// Border style for the active panel in Panel nav mode — brighter and bold.
    pub fn panel_focused_border_style(&self) -> Style {
        Style::default()
            .fg(self.colors.warning)
            .add_modifier(Modifier::BOLD)
    }

    pub fn unfocused_border_style(&self) -> Style {
        Style::default().fg(self.colors.muted)
    }

    pub fn selected_style(&self) -> Style {
        Style::default()
            .fg(self.colors.foreground)
            .bg(self.colors.accent)
    }

    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.colors.muted)
    }

    pub fn accent_style(&self) -> Style {
        Style::default().fg(self.colors.accent)
    }

    pub fn error_style(&self) -> Style {
        Style::default().fg(self.colors.error)
    }

    pub fn success_style(&self) -> Style {
        Style::default().fg(self.colors.success)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.colors.warning)
    }

    pub fn border_set(&self) -> ratatui::symbols::border::Set {
        match self.border_style {
            BorderStyle::Rounded => ratatui::symbols::border::ROUNDED,
            BorderStyle::Plain => ratatui::symbols::border::PLAIN,
            BorderStyle::Double => ratatui::symbols::border::DOUBLE,
            BorderStyle::Thick => ratatui::symbols::border::THICK,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin()
    }
}

fn hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    Color::Rgb(r, g, b)
}
