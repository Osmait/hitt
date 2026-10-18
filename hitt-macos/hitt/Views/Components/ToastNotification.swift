import SwiftUI

struct ToastNotification: View {
    let message: String
    let isError: Bool

    var body: some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            Image(systemName: isError ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                .foregroundStyle(isError ? HittTheme.Colors.danger : HittTheme.Colors.success)
            Text(message)
                .font(HittTheme.Typography.bodyMedium)
                .foregroundStyle(HittTheme.Colors.textPrimary)
        }
        .padding(.horizontal, HittTheme.Spacing.lg)
        .padding(.vertical, HittTheme.Spacing.sm)
        .background(.ultraThinMaterial)
        .background(HittTheme.Colors.surfaceElevated.opacity(0.9))
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.large))
        .shadow(color: .black.opacity(0.3), radius: 20, y: 8)
    }
}
