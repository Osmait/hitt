use anyhow::Result;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

use crate::core::request::HttpMethod;

// ---------------------------------------------------------------------------
// Override structs — all fields optional, deserialized from config.toml
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeOverride {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub accent: Option<String>,
    pub success: Option<String>,
    pub warning: Option<String>,
    pub error: Option<String>,
    pub muted: Option<String>,
    #[serde(default)]
    pub methods: Option<MethodColorsOverride>,
    #[serde(default)]
    pub status: Option<StatusColorsOverride>,
    #[serde(default)]
    pub syntax: Option<SyntaxColorsOverride>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MethodColorsOverride {
    pub get: Option<String>,
    pub post: Option<String>,
    pub put: Option<String>,
    pub patch: Option<String>,
    pub delete: Option<String>,
    pub head: Option<String>,
    pub options: Option<String>,
    pub ws: Option<String>,
    pub sse: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusColorsOverride {
    pub info: Option<String>,
    pub success: Option<String>,
    pub redirect: Option<String>,
    pub client_error: Option<String>,
    pub server_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyntaxColorsOverride {
    pub json_key: Option<String>,
    pub json_string: Option<String>,
    pub json_number: Option<String>,
    pub json_boolean: Option<String>,
    pub json_null: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BorderOverride {
    pub style: Option<String>,
    pub sidebar_width: Option<u16>,
}

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

pub const AVAILABLE_THEMES: &[&str] = &[
    "catppuccin",
    "dracula",
    "everforest",
    "gruvbox",
    "kanagawa",
    "nord",
    "one-dark",
    "rose-pine",
    "solarized-dark",
    "tokyo-night",
];

impl Theme {
    pub fn load(name: &str) -> Result<Self> {
        match name {
            "catppuccin" => Ok(Self::catppuccin()),
            "dracula" => Ok(Self::dracula()),
            "everforest" => Ok(Self::everforest()),
            "gruvbox" => Ok(Self::gruvbox()),
            "kanagawa" => Ok(Self::kanagawa()),
            "nord" => Ok(Self::nord()),
            "one-dark" => Ok(Self::one_dark()),
            "rose-pine" => Ok(Self::rose_pine()),
            "solarized-dark" => Ok(Self::solarized_dark()),
            "tokyo-night" => Ok(Self::tokyo_night()),
            _ => anyhow::bail!("Unknown theme: {name}"),
        }
    }

    /// Catppuccin Mocha — official palette from catppuccin.com
    pub fn catppuccin() -> Self {
        // Catppuccin Mocha palette (catppuccin.com)
        let base     = "#1E1E2E";
        let overlay0 = "#6C7086";
        let text     = "#CDD6F4";
        let pink     = "#F5C2E7";
        let mauve    = "#CBA6F7";
        let red      = "#F38BA8";
        let peach    = "#FAB387";
        let yellow   = "#F9E2AF";
        let green    = "#A6E3A1";
        let teal     = "#94E2D5";
        let sky      = "#89DCEB";
        let blue     = "#89B4FA";
        let lavender = "#B4BEFE";

        Self {
            name: "catppuccin".to_string(),
            colors: ThemeColors {
                background: hex_color(base),
                foreground: hex_color(text),
                accent: hex_color(blue),
                success: hex_color(green),
                warning: hex_color(yellow),
                error: hex_color(red),
                muted: hex_color(overlay0),
                methods: MethodColors {
                    get: hex_color(green),
                    post: hex_color(yellow),
                    put: hex_color(blue),
                    patch: hex_color(mauve),
                    delete: hex_color(red),
                    head: hex_color(teal),
                    options: hex_color(pink),
                    ws: hex_color(lavender),
                    sse: hex_color(sky),
                },
                status: StatusColors {
                    info: hex_color(blue),
                    success: hex_color(green),
                    redirect: hex_color(yellow),
                    client_error: hex_color(peach),
                    server_error: hex_color(red),
                },
                syntax: SyntaxColors {
                    json_key: hex_color(blue),
                    json_string: hex_color(green),
                    json_number: hex_color(peach),
                    json_boolean: hex_color(mauve),
                    json_null: hex_color(overlay0),
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

    /// Nord — official palette from nordtheme.com
    pub fn nord() -> Self {
        let polar0  = "#2E3440"; // bg
        let polar3  = "#4C566A"; // muted
        let snow0   = "#D8DEE9"; // fg
        let frost0  = "#8FBCBB"; // teal
        let frost1  = "#88C0D0"; // cyan
        let frost2  = "#81A1C1"; // blue
        let frost3  = "#5E81AC"; // dark blue
        let aurora0 = "#BF616A"; // red
        let aurora1 = "#D08770"; // orange
        let aurora2 = "#EBCB8B"; // yellow
        let aurora3 = "#A3BE8C"; // green
        let aurora4 = "#B48EAD"; // purple

        Self {
            name: "nord".to_string(),
            colors: ThemeColors {
                background: hex_color(polar0),
                foreground: hex_color(snow0),
                accent: hex_color(frost2),
                success: hex_color(aurora3),
                warning: hex_color(aurora2),
                error: hex_color(aurora0),
                muted: hex_color(polar3),
                methods: MethodColors {
                    get: hex_color(aurora3),
                    post: hex_color(aurora2),
                    put: hex_color(frost2),
                    patch: hex_color(aurora4),
                    delete: hex_color(aurora0),
                    head: hex_color(frost0),
                    options: hex_color(aurora1),
                    ws: hex_color(frost3),
                    sse: hex_color(frost1),
                },
                status: StatusColors {
                    info: hex_color(frost1),
                    success: hex_color(aurora3),
                    redirect: hex_color(aurora2),
                    client_error: hex_color(aurora1),
                    server_error: hex_color(aurora0),
                },
                syntax: SyntaxColors {
                    json_key: hex_color(frost2),
                    json_string: hex_color(aurora3),
                    json_number: hex_color(aurora4),
                    json_boolean: hex_color(aurora1),
                    json_null: hex_color(polar3),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    /// Solarized Dark — official palette from ethanschoonover.com/solarized
    pub fn solarized_dark() -> Self {
        let base03  = "#002B36"; // bg
        let base01  = "#586E75"; // muted
        let base0   = "#839496"; // fg
        let yellow  = "#B58900";
        let orange  = "#CB4B16";
        let red     = "#DC322F";
        let magenta = "#D33682";
        let violet  = "#6C71C4";
        let blue    = "#268BD2";
        let cyan    = "#2AA198";
        let green   = "#859900";

        Self {
            name: "solarized-dark".to_string(),
            colors: ThemeColors {
                background: hex_color(base03),
                foreground: hex_color(base0),
                accent: hex_color(blue),
                success: hex_color(green),
                warning: hex_color(yellow),
                error: hex_color(red),
                muted: hex_color(base01),
                methods: MethodColors {
                    get: hex_color(green),
                    post: hex_color(yellow),
                    put: hex_color(blue),
                    patch: hex_color(violet),
                    delete: hex_color(red),
                    head: hex_color(cyan),
                    options: hex_color(magenta),
                    ws: hex_color(violet),
                    sse: hex_color(cyan),
                },
                status: StatusColors {
                    info: hex_color(blue),
                    success: hex_color(green),
                    redirect: hex_color(yellow),
                    client_error: hex_color(orange),
                    server_error: hex_color(red),
                },
                syntax: SyntaxColors {
                    json_key: hex_color(blue),
                    json_string: hex_color(cyan),
                    json_number: hex_color(magenta),
                    json_boolean: hex_color(violet),
                    json_null: hex_color(base01),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    /// One Dark — palette from Atom One Dark theme
    pub fn one_dark() -> Self {
        let bg      = "#282C34";
        let fg      = "#ABB2BF";
        let comment = "#5C6370";
        let red     = "#E06C75";
        let green   = "#98C379";
        let yellow  = "#E5C07B";
        let blue    = "#61AFEF";
        let magenta = "#C678DD";
        let cyan    = "#56B6C2";
        let orange  = "#D19A66";

        Self {
            name: "one-dark".to_string(),
            colors: ThemeColors {
                background: hex_color(bg),
                foreground: hex_color(fg),
                accent: hex_color(blue),
                success: hex_color(green),
                warning: hex_color(yellow),
                error: hex_color(red),
                muted: hex_color(comment),
                methods: MethodColors {
                    get: hex_color(green),
                    post: hex_color(yellow),
                    put: hex_color(blue),
                    patch: hex_color(magenta),
                    delete: hex_color(red),
                    head: hex_color(cyan),
                    options: hex_color(orange),
                    ws: hex_color(magenta),
                    sse: hex_color(cyan),
                },
                status: StatusColors {
                    info: hex_color(blue),
                    success: hex_color(green),
                    redirect: hex_color(yellow),
                    client_error: hex_color(orange),
                    server_error: hex_color(red),
                },
                syntax: SyntaxColors {
                    json_key: hex_color(blue),
                    json_string: hex_color(green),
                    json_number: hex_color(orange),
                    json_boolean: hex_color(magenta),
                    json_null: hex_color(comment),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    /// Kanagawa — palette from rebelot/kanagawa.nvim
    pub fn kanagawa() -> Self {
        let sumi_ink3   = "#1F1F28"; // bg
        let fuji_gray   = "#727169"; // muted
        let fuji_white  = "#DCD7BA"; // fg
        let spring_blue = "#7FB4CA";
        let crystal_blue = "#7E9CD8";
        let spring_green = "#98BB6C";
        let carp_yellow  = "#E6C384";
        let autumn_red  = "#C34043";
        let surimi_orange = "#FFA066";
        let oni_violet  = "#957FB8";
        let sakura_pink = "#D27E99";
        let wave_aqua   = "#6A9589";

        Self {
            name: "kanagawa".to_string(),
            colors: ThemeColors {
                background: hex_color(sumi_ink3),
                foreground: hex_color(fuji_white),
                accent: hex_color(crystal_blue),
                success: hex_color(spring_green),
                warning: hex_color(carp_yellow),
                error: hex_color(autumn_red),
                muted: hex_color(fuji_gray),
                methods: MethodColors {
                    get: hex_color(spring_green),
                    post: hex_color(carp_yellow),
                    put: hex_color(crystal_blue),
                    patch: hex_color(oni_violet),
                    delete: hex_color(autumn_red),
                    head: hex_color(wave_aqua),
                    options: hex_color(sakura_pink),
                    ws: hex_color(spring_blue),
                    sse: hex_color(wave_aqua),
                },
                status: StatusColors {
                    info: hex_color(crystal_blue),
                    success: hex_color(spring_green),
                    redirect: hex_color(carp_yellow),
                    client_error: hex_color(surimi_orange),
                    server_error: hex_color(autumn_red),
                },
                syntax: SyntaxColors {
                    json_key: hex_color(crystal_blue),
                    json_string: hex_color(spring_green),
                    json_number: hex_color(sakura_pink),
                    json_boolean: hex_color(oni_violet),
                    json_null: hex_color(fuji_gray),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    /// Rosé Pine — palette from rosepinetheme.com
    pub fn rose_pine() -> Self {
        let base    = "#191724";
        let muted   = "#6E6A86";
        let text    = "#E0DEF4";
        let love    = "#EB6F92"; // red
        let gold    = "#F6C177"; // yellow
        let rose    = "#EBBCBA"; // pink
        let pine    = "#31748F"; // teal
        let foam    = "#9CCFD8"; // cyan
        let iris    = "#C4A7E7"; // purple

        Self {
            name: "rose-pine".to_string(),
            colors: ThemeColors {
                background: hex_color(base),
                foreground: hex_color(text),
                accent: hex_color(iris),
                success: hex_color(foam),
                warning: hex_color(gold),
                error: hex_color(love),
                muted: hex_color(muted),
                methods: MethodColors {
                    get: hex_color(foam),
                    post: hex_color(gold),
                    put: hex_color(iris),
                    patch: hex_color(rose),
                    delete: hex_color(love),
                    head: hex_color(pine),
                    options: hex_color(rose),
                    ws: hex_color(iris),
                    sse: hex_color(foam),
                },
                status: StatusColors {
                    info: hex_color(foam),
                    success: hex_color(pine),
                    redirect: hex_color(gold),
                    client_error: hex_color(gold),
                    server_error: hex_color(love),
                },
                syntax: SyntaxColors {
                    json_key: hex_color(foam),
                    json_string: hex_color(gold),
                    json_number: hex_color(iris),
                    json_boolean: hex_color(rose),
                    json_null: hex_color(muted),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    /// Everforest Dark — palette from sainnhe/everforest
    pub fn everforest() -> Self {
        let bg0     = "#2D353B";
        let grey1   = "#859289";
        let fg      = "#D3C6AA";
        let red     = "#E67E80";
        let orange  = "#E69875";
        let yellow  = "#DBBC7F";
        let green   = "#A7C080";
        let aqua    = "#83C092";
        let blue    = "#7FBBB3";
        let purple  = "#D699B6";

        Self {
            name: "everforest".to_string(),
            colors: ThemeColors {
                background: hex_color(bg0),
                foreground: hex_color(fg),
                accent: hex_color(blue),
                success: hex_color(green),
                warning: hex_color(yellow),
                error: hex_color(red),
                muted: hex_color(grey1),
                methods: MethodColors {
                    get: hex_color(green),
                    post: hex_color(yellow),
                    put: hex_color(blue),
                    patch: hex_color(purple),
                    delete: hex_color(red),
                    head: hex_color(aqua),
                    options: hex_color(orange),
                    ws: hex_color(purple),
                    sse: hex_color(aqua),
                },
                status: StatusColors {
                    info: hex_color(blue),
                    success: hex_color(green),
                    redirect: hex_color(yellow),
                    client_error: hex_color(orange),
                    server_error: hex_color(red),
                },
                syntax: SyntaxColors {
                    json_key: hex_color(blue),
                    json_string: hex_color(green),
                    json_number: hex_color(purple),
                    json_boolean: hex_color(orange),
                    json_null: hex_color(grey1),
                },
            },
            border_style: BorderStyle::Rounded,
            sidebar_width: 25,
        }
    }

    /// Apply partial color overrides from config. Only `Some` fields are applied.
    pub fn apply_overrides(&mut self, overrides: &ThemeOverride) {
        if let Some(ref v) = overrides.background {
            self.colors.background = try_hex_color(v, self.colors.background);
        }
        if let Some(ref v) = overrides.foreground {
            self.colors.foreground = try_hex_color(v, self.colors.foreground);
        }
        if let Some(ref v) = overrides.accent {
            self.colors.accent = try_hex_color(v, self.colors.accent);
        }
        if let Some(ref v) = overrides.success {
            self.colors.success = try_hex_color(v, self.colors.success);
        }
        if let Some(ref v) = overrides.warning {
            self.colors.warning = try_hex_color(v, self.colors.warning);
        }
        if let Some(ref v) = overrides.error {
            self.colors.error = try_hex_color(v, self.colors.error);
        }
        if let Some(ref v) = overrides.muted {
            self.colors.muted = try_hex_color(v, self.colors.muted);
        }
        if let Some(ref methods) = overrides.methods {
            if let Some(ref v) = methods.get {
                self.colors.methods.get = try_hex_color(v, self.colors.methods.get);
            }
            if let Some(ref v) = methods.post {
                self.colors.methods.post = try_hex_color(v, self.colors.methods.post);
            }
            if let Some(ref v) = methods.put {
                self.colors.methods.put = try_hex_color(v, self.colors.methods.put);
            }
            if let Some(ref v) = methods.patch {
                self.colors.methods.patch = try_hex_color(v, self.colors.methods.patch);
            }
            if let Some(ref v) = methods.delete {
                self.colors.methods.delete = try_hex_color(v, self.colors.methods.delete);
            }
            if let Some(ref v) = methods.head {
                self.colors.methods.head = try_hex_color(v, self.colors.methods.head);
            }
            if let Some(ref v) = methods.options {
                self.colors.methods.options = try_hex_color(v, self.colors.methods.options);
            }
            if let Some(ref v) = methods.ws {
                self.colors.methods.ws = try_hex_color(v, self.colors.methods.ws);
            }
            if let Some(ref v) = methods.sse {
                self.colors.methods.sse = try_hex_color(v, self.colors.methods.sse);
            }
        }
        if let Some(ref status) = overrides.status {
            if let Some(ref v) = status.info {
                self.colors.status.info = try_hex_color(v, self.colors.status.info);
            }
            if let Some(ref v) = status.success {
                self.colors.status.success = try_hex_color(v, self.colors.status.success);
            }
            if let Some(ref v) = status.redirect {
                self.colors.status.redirect = try_hex_color(v, self.colors.status.redirect);
            }
            if let Some(ref v) = status.client_error {
                self.colors.status.client_error = try_hex_color(v, self.colors.status.client_error);
            }
            if let Some(ref v) = status.server_error {
                self.colors.status.server_error = try_hex_color(v, self.colors.status.server_error);
            }
        }
        if let Some(ref syntax) = overrides.syntax {
            if let Some(ref v) = syntax.json_key {
                self.colors.syntax.json_key = try_hex_color(v, self.colors.syntax.json_key);
            }
            if let Some(ref v) = syntax.json_string {
                self.colors.syntax.json_string = try_hex_color(v, self.colors.syntax.json_string);
            }
            if let Some(ref v) = syntax.json_number {
                self.colors.syntax.json_number = try_hex_color(v, self.colors.syntax.json_number);
            }
            if let Some(ref v) = syntax.json_boolean {
                self.colors.syntax.json_boolean =
                    try_hex_color(v, self.colors.syntax.json_boolean);
            }
            if let Some(ref v) = syntax.json_null {
                self.colors.syntax.json_null = try_hex_color(v, self.colors.syntax.json_null);
            }
        }
    }

    /// Apply partial border overrides from config.
    pub fn apply_border_overrides(&mut self, overrides: &BorderOverride) {
        if let Some(ref style) = overrides.style {
            self.border_style = match style.to_lowercase().as_str() {
                "rounded" => BorderStyle::Rounded,
                "plain" => BorderStyle::Plain,
                "double" => BorderStyle::Double,
                "thick" => BorderStyle::Thick,
                _ => self.border_style,
            };
        }
        if let Some(width) = overrides.sidebar_width {
            self.sidebar_width = width;
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
        Style::default()
            .fg(self.colors.methods.ws)
            .add_modifier(Modifier::BOLD)
    }

    pub fn protocol_style_sse(&self) -> Style {
        Style::default()
            .fg(self.colors.methods.sse)
            .add_modifier(Modifier::BOLD)
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

/// Parse a hex color string, returning `fallback` if the string is invalid.
fn try_hex_color(hex: &str, fallback: Color) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return fallback;
    }
    let Ok(r) = u8::from_str_radix(&hex[0..2], 16) else {
        return fallback;
    };
    let Ok(g) = u8::from_str_radix(&hex[2..4], 16) else {
        return fallback;
    };
    let Ok(b) = u8::from_str_radix(&hex[4..6], 16) else {
        return fallback;
    };
    Color::Rgb(r, g, b)
}
