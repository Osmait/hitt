import SwiftUI

struct ResponseHeadersView: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        if let resp = appVM.response.lastResponse {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(resp.headers) { kv in
                        HStack(alignment: .top, spacing: HittTheme.Spacing.md) {
                            Text(kv.key)
                                .font(HittTheme.Typography.monoBold)
                                .foregroundStyle(HittTheme.Colors.accent)
                                .frame(width: 200, alignment: .leading)
                            Text(kv.value)
                                .font(HittTheme.Typography.mono)
                                .foregroundStyle(HittTheme.Colors.textPrimary)
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .padding(.horizontal, HittTheme.Spacing.md)
                        .padding(.vertical, 5)
                        Divider().background(HittTheme.Colors.borderSubtle.opacity(0.3))
                    }
                }
            }
        } else {
            EmptyStateView(icon: "list.bullet", title: "No headers")
        }
    }
}
