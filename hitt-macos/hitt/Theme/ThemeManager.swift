import SwiftUI

/// Persisted theme selection.
enum ThemeName: String, CaseIterable, Identifiable {
    case mocha = "Catppuccin Mocha"
    case dracula = "Dracula"
    case tokyoNight = "Tokyo Night"
    case oneDark = "One Dark Pro"
    case gruvbox = "Gruvbox Dark"
    case rosePine = "Rosé Pine"

    var id: String { rawValue }
}

// MARK: - Palette sub-structs

struct ChromeColors {
    let background: Color
    let surface: Color
    let surfaceElevated: Color
    let surfaceHover: Color
    let border: Color
    let borderSubtle: Color
}

struct AccentColors {
    let accent: Color
    let accentHover: Color
    let danger: Color
    let success: Color
    let warning: Color
}

struct TextColors {
    let primary: Color
    let secondary: Color
    let tertiary: Color
}

struct MethodColors {
    let get: Color
    let post: Color
    let put: Color
    let patch: Color
    let delete: Color
    let head: Color
    let options: Color
}

/// Full color palette for a single theme.
struct ThemePalette {
    let chrome: ChromeColors
    let accents: AccentColors
    let text: TextColors
    let methods: MethodColors

    // Flat accessors
    var background: Color { chrome.background }
    var surface: Color { chrome.surface }
    var surfaceElevated: Color { chrome.surfaceElevated }
    var surfaceHover: Color { chrome.surfaceHover }
    var border: Color { chrome.border }
    var borderSubtle: Color { chrome.borderSubtle }

    var accent: Color { accents.accent }
    var accentHover: Color { accents.accentHover }
    var danger: Color { accents.danger }
    var success: Color { accents.success }
    var warning: Color { accents.warning }

    var textPrimary: Color { text.primary }
    var textSecondary: Color { text.secondary }
    var textTertiary: Color { text.tertiary }

    var selection: Color { accent.opacity(0.15) }

    var methodGet: Color { methods.get }
    var methodPost: Color { methods.post }
    var methodPut: Color { methods.put }
    var methodPatch: Color { methods.patch }
    var methodDelete: Color { methods.delete }
    var methodHead: Color { methods.head }
    var methodOptions: Color { methods.options }
}

// Local helper so the type-checker doesn't chew on long Color(hex:) chains.
@inline(__always) private func c(_ hex: UInt32) -> Color { Color(hex: hex) }

/// Global @Observable providing the active palette.
@Observable
final class ThemeManager {
    static let shared = ThemeManager()

    private static let storageKey = "selectedTheme"

    var activeThemeName: ThemeName {
        didSet {
            UserDefaults.standard.set(activeThemeName.rawValue, forKey: Self.storageKey)
        }
    }

    var palette: ThemePalette {
        Self.palette(for: activeThemeName)
    }

    private init() {
        if let stored = UserDefaults.standard.string(forKey: Self.storageKey),
           let name = ThemeName(rawValue: stored)
        {
            activeThemeName = name
        } else {
            activeThemeName = .mocha
        }
    }

    static func palette(for theme: ThemeName) -> ThemePalette {
        switch theme {
        case .mocha: mochaPalette
        case .dracula: draculaPalette
        case .tokyoNight: tokyoNightPalette
        case .oneDark: oneDarkPalette
        case .gruvbox: gruvboxPalette
        case .rosePine: rosePinePalette
        }
    }

    // MARK: - Palettes (each built step-by-step to keep the type-checker happy)

    private static let mochaPalette: ThemePalette = {
        let chrome = ChromeColors(
            background: c(0x11111B), surface: c(0x1E1E2E),
            surfaceElevated: c(0x313244), surfaceHover: c(0x45475A),
            border: c(0x45475A), borderSubtle: c(0x313244)
        )
        let accents = AccentColors(
            accent: c(0xCBA6F7), accentHover: c(0xB4BEFE),
            danger: c(0xF38BA8), success: c(0xA6E3A1), warning: c(0xF9E2AF)
        )
        let text = TextColors(
            primary: c(0xCDD6F4), secondary: c(0xA6ADC8), tertiary: c(0x8E93AB)
        )
        let methods = MethodColors(
            get: c(0xA6E3A1), post: c(0xF9E2AF), put: c(0x89B4FA),
            patch: c(0xCBA6F7), delete: c(0xF38BA8),
            head: c(0x94E2D5), options: c(0xFAB387)
        )
        return ThemePalette(chrome: chrome, accents: accents, text: text, methods: methods)
    }()

    private static let draculaPalette: ThemePalette = {
        let chrome = ChromeColors(
            background: c(0x282A36), surface: c(0x2D303D),
            surfaceElevated: c(0x343746), surfaceHover: c(0x44475A),
            border: c(0x44475A), borderSubtle: c(0x343746)
        )
        let accents = AccentColors(
            accent: c(0xBD93F9), accentHover: c(0xFF79C6),
            danger: c(0xFF5555), success: c(0x50FA7B), warning: c(0xF1FA8C)
        )
        let text = TextColors(
            primary: c(0xF8F8F2), secondary: c(0xBDBFC6), tertiary: c(0x7E8DB8)
        )
        let methods = MethodColors(
            get: c(0x50FA7B), post: c(0xF1FA8C), put: c(0x8BE9FD),
            patch: c(0xBD93F9), delete: c(0xFF5555),
            head: c(0xFF79C6), options: c(0xFFB86C)
        )
        return ThemePalette(chrome: chrome, accents: accents, text: text, methods: methods)
    }()

    private static let tokyoNightPalette: ThemePalette = {
        let chrome = ChromeColors(
            background: c(0x16161E), surface: c(0x1A1B26),
            surfaceElevated: c(0x24283B), surfaceHover: c(0x292E42),
            border: c(0x292E42), borderSubtle: c(0x24283B)
        )
        let accents = AccentColors(
            accent: c(0x7AA2F7), accentHover: c(0xBB9AF7),
            danger: c(0xF7768E), success: c(0x9ECE6A), warning: c(0xE0AF68)
        )
        let text = TextColors(
            primary: c(0xA9B1D6), secondary: c(0x9099B7), tertiary: c(0x6B7394)
        )
        let methods = MethodColors(
            get: c(0x9ECE6A), post: c(0xE0AF68), put: c(0x7AA2F7),
            patch: c(0xBB9AF7), delete: c(0xF7768E),
            head: c(0x73DACA), options: c(0xFF9E64)
        )
        return ThemePalette(chrome: chrome, accents: accents, text: text, methods: methods)
    }()

    private static let oneDarkPalette: ThemePalette = {
        let chrome = ChromeColors(
            background: c(0x21252B), surface: c(0x282C34),
            surfaceElevated: c(0x2C313A), surfaceHover: c(0x3E4451),
            border: c(0x3E4451), borderSubtle: c(0x2C313A)
        )
        let accents = AccentColors(
            accent: c(0xC678DD), accentHover: c(0x61AFEF),
            danger: c(0xE06C75), success: c(0x98C379), warning: c(0xE5C07B)
        )
        let text = TextColors(
            primary: c(0xABB2BF), secondary: c(0x9DA2AC), tertiary: c(0x7A808C)
        )
        let methods = MethodColors(
            get: c(0x98C379), post: c(0xE5C07B), put: c(0x61AFEF),
            patch: c(0xC678DD), delete: c(0xE06C75),
            head: c(0x56B6C2), options: c(0xD19A66)
        )
        return ThemePalette(chrome: chrome, accents: accents, text: text, methods: methods)
    }()

    private static let gruvboxPalette: ThemePalette = {
        let chrome = ChromeColors(
            background: c(0x1D2021), surface: c(0x282828),
            surfaceElevated: c(0x3C3836), surfaceHover: c(0x504945),
            border: c(0x504945), borderSubtle: c(0x3C3836)
        )
        let accents = AccentColors(
            accent: c(0xFE8019), accentHover: c(0xFABD2F),
            danger: c(0xFB4934), success: c(0xB8BB26), warning: c(0xFABD2F)
        )
        let text = TextColors(
            primary: c(0xEBDBB2), secondary: c(0xBDAE93), tertiary: c(0xA89984)
        )
        let methods = MethodColors(
            get: c(0xB8BB26), post: c(0xFABD2F), put: c(0x83A598),
            patch: c(0xD3869B), delete: c(0xFB4934),
            head: c(0x8EC07C), options: c(0xFE8019)
        )
        return ThemePalette(chrome: chrome, accents: accents, text: text, methods: methods)
    }()

    private static let rosePinePalette: ThemePalette = {
        let chrome = ChromeColors(
            background: c(0x191724), surface: c(0x1F1D2E),
            surfaceElevated: c(0x26233A), surfaceHover: c(0x312D45),
            border: c(0x312D45), borderSubtle: c(0x26233A)
        )
        let accents = AccentColors(
            accent: c(0xEBBCBA), accentHover: c(0xC4A7E7),
            danger: c(0xEB6F92), success: c(0x9CCFD8), warning: c(0xF6C177)
        )
        let text = TextColors(
            primary: c(0xE0DEF4), secondary: c(0xC4C0DB), tertiary: c(0x908CAA)
        )
        let methods = MethodColors(
            get: c(0x9CCFD8), post: c(0xF6C177), put: c(0xC4A7E7),
            patch: c(0xEBBCBA), delete: c(0xEB6F92),
            head: c(0x31748F), options: c(0xEBBCBA)
        )
        return ThemePalette(chrome: chrome, accents: accents, text: text, methods: methods)
    }()
}
