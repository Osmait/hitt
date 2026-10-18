import SwiftUI

/// Colored pill showing an HTTP status code.
struct StatusBadge: View {
    let status: UInt16
    let text: String?

    var body: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(color)
                .frame(width: 7, height: 7)
            Text("\(status)")
                .font(HittTheme.Typography.captionBold)
                .monospaced()
                .foregroundStyle(HittTheme.Colors.textPrimary)
            if let t = text, !t.isEmpty {
                Text(t)
                    .font(HittTheme.Typography.caption)
                    .foregroundStyle(HittTheme.Colors.textSecondary)
            }
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 2)
        .background(color.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }

    private var color: Color {
        switch status {
        case 100..<200: HittTheme.Colors.textSecondary
        case 200..<300: HittTheme.Colors.success
        case 300..<400: HittTheme.Colors.warning
        case 400..<600: HittTheme.Colors.danger
        default: HittTheme.Colors.textTertiary
        }
    }
}
