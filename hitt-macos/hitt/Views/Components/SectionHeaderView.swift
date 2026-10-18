import SwiftUI

struct SectionHeaderView: View {
    let title: String
    var action: (() -> Void)? = nil
    var actionIcon: String = "plus"

    var body: some View {
        HStack {
            Text(title)
                .font(HittTheme.Typography.tiny)
                .tracking(0.5)
                .foregroundStyle(HittTheme.Colors.textTertiary)
            Spacer()
            if let action {
                Button(action: action) {
                    Image(systemName: actionIcon)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(HittTheme.Colors.textSecondary)
                }
                .buttonStyle(.hoverIcon)
            }
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, HittTheme.Spacing.xs)
    }
}
