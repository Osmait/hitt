import SwiftUI

struct SearchFieldView: View {
    @Binding var text: String
    var placeholder: String = "Search"

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11))
                .foregroundStyle(HittTheme.Colors.textTertiary)
            TextField(placeholder, text: $text)
                .textFieldStyle(.plain)
                .font(HittTheme.Typography.body)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            if !text.isEmpty {
                Button { text = "" } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 11))
                        .foregroundStyle(HittTheme.Colors.textTertiary)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 6)
        .background(HittTheme.Colors.background.opacity(0.6))
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }
}
