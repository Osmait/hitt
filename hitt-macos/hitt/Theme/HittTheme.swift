import SwiftUI

/// Design tokens for the hitt macOS app.
/// Colors resolve dynamically from the active ThemeManager palette.
enum HittTheme {
    // MARK: - Colors

    enum Colors {
        private static var p: ThemePalette { ThemeManager.shared.palette }

        static var background: Color { p.background }
        static var surface: Color { p.surface }
        static var surfaceElevated: Color { p.surfaceElevated }
        static var surfaceHover: Color { p.surfaceHover }
        static var accent: Color { p.accent }
        static var accentHover: Color { p.accentHover }
        static var danger: Color { p.danger }
        static var success: Color { p.success }
        static var warning: Color { p.warning }
        static var textPrimary: Color { p.textPrimary }
        static var textSecondary: Color { p.textSecondary }
        static var textTertiary: Color { p.textTertiary }
        static var border: Color { p.border }
        static var borderSubtle: Color { p.borderSubtle }
        static var selection: Color { p.selection }

        // HTTP method colors
        static var methodGet: Color { p.methodGet }
        static var methodPost: Color { p.methodPost }
        static var methodPut: Color { p.methodPut }
        static var methodPatch: Color { p.methodPatch }
        static var methodDelete: Color { p.methodDelete }
        static var methodHead: Color { p.methodHead }
        static var methodOptions: Color { p.methodOptions }
    }

    // MARK: - Corner Radii

    enum Radius {
        static let small: CGFloat = 4
        static let medium: CGFloat = 8
        static let large: CGFloat = 12
        static let island: CGFloat = 10
    }

    // MARK: - Spacing (4pt grid)

    enum Spacing {
        static let xxs: CGFloat = 2
        static let xs: CGFloat = 4
        static let sm: CGFloat = 8
        static let md: CGFloat = 12
        static let lg: CGFloat = 16
        static let xl: CGFloat = 24
        static let xxl: CGFloat = 32
    }

    // MARK: - Sizing

    enum Size {
        static let sidebarWidth: CGFloat = 260
        static let rowHeight: CGFloat = 28
        static let urlBarHeight: CGFloat = 36
        static let headerHeight: CGFloat = 40
    }
}

// MARK: - Color hex initializer

extension Color {
    init(hex: UInt32, opacity: Double = 1.0) {
        let r = Double((hex >> 16) & 0xFF) / 255.0
        let g = Double((hex >> 8) & 0xFF) / 255.0
        let b = Double(hex & 0xFF) / 255.0
        self.init(.sRGB, red: r, green: g, blue: b, opacity: opacity)
    }
}
