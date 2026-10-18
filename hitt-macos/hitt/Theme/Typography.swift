import SwiftUI

extension HittTheme {
    enum Typography {
        // Sans
        static let title = Font.system(size: 18, weight: .semibold, design: .default)
        static let headline = Font.system(size: 14, weight: .semibold, design: .default)
        static let body = Font.system(size: 13, weight: .regular, design: .default)
        static let bodyMedium = Font.system(size: 13, weight: .medium, design: .default)
        static let caption = Font.system(size: 11, weight: .regular, design: .default)
        static let captionBold = Font.system(size: 11, weight: .semibold, design: .default)
        static let tiny = Font.system(size: 10, weight: .medium, design: .default)

        // Mono — response bodies, URL, headers
        static let mono = Font.system(size: 12, weight: .regular, design: .monospaced)
        static let monoSmall = Font.system(size: 11, weight: .regular, design: .monospaced)
        static let monoBold = Font.system(size: 12, weight: .semibold, design: .monospaced)
    }
}
