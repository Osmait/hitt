import SwiftUI

struct EmptyStateView: View {
    let icon: String
    let title: String
    var subtitle: String? = nil

    var body: some View {
        VStack(spacing: HittTheme.Spacing.sm) {
            Image(systemName: icon)
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(HittTheme.Colors.textTertiary)
            Text(title)
                .font(HittTheme.Typography.bodyMedium)
                .foregroundStyle(HittTheme.Colors.textSecondary)
            if let subtitle {
                Text(subtitle)
                    .font(HittTheme.Typography.caption)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
                    .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }
}
