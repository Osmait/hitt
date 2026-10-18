import SwiftUI

/// Colored pill showing an HTTP method.
struct MethodBadge: View {
    let method: HttpMethod
    var style: Style = .filled

    enum Style { case filled, outline, text }

    var body: some View {
        let color = method.color
        Text(method.rawValue)
            .font(HittTheme.Typography.captionBold)
            .monospaced()
            .foregroundStyle(style == .filled ? Color.black : color)
            .padding(.horizontal, HittTheme.Spacing.sm)
            .padding(.vertical, 2)
            .background(
                Group {
                    switch style {
                    case .filled: color
                    case .outline: color.opacity(0.15)
                    case .text: Color.clear
                    }
                }
            )
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }
}
