import SwiftUI

struct ResponseToolbar: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        @Bindable var response = appVM.response
        HStack(spacing: HittTheme.Spacing.md) {
            // Status
            if let ffi = response.lastFfi {
                StatusBadge(status: ffi.status, text: ffi.statusText)
                metric(icon: "bolt.fill", color: timeColor(ffi.timeMs), text: "\(ffi.timeMs) ms")
                metric(icon: "doc.fill", color: HittTheme.Colors.textSecondary, text: sizeString(ffi.sizeBytes))
            } else {
                Text("No response yet")
                    .font(HittTheme.Typography.caption)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }

            Spacer()

            // Tab picker
            if response.lastFfi != nil {
                HStack(spacing: 2) {
                    ForEach(ResponseViewModel.ResponseTab.allCases) { tab in
                        Button {
                            response.selectedTab = tab
                        } label: {
                            Text(tab.rawValue)
                                .font(HittTheme.Typography.captionBold)
                                .foregroundStyle(
                                    response.selectedTab == tab
                                        ? HittTheme.Colors.textPrimary
                                        : HittTheme.Colors.textTertiary
                                )
                                .padding(.horizontal, HittTheme.Spacing.sm)
                                .padding(.vertical, 3)
                                .background(
                                    response.selectedTab == tab
                                        ? HittTheme.Colors.surfaceElevated
                                        : Color.clear
                                )
                                .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
                        }
                        .buttonStyle(.hover)
                    }
                }
            }
        }
        .padding(.horizontal, HittTheme.Spacing.md)
        .padding(.vertical, HittTheme.Spacing.sm)
        .background(HittTheme.Colors.surface)
    }

    private func metric(icon: String, color: Color, text: String) -> some View {
        HStack(spacing: 4) {
            Image(systemName: icon).font(.system(size: 9))
            Text(text)
                .font(HittTheme.Typography.captionBold)
                .monospaced()
        }
        .foregroundStyle(color)
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 2)
        .background(color.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }

    private func timeColor(_ ms: UInt64) -> Color {
        switch ms {
        case 0..<500: HittTheme.Colors.success
        case 500..<2000: HittTheme.Colors.warning
        default: HittTheme.Colors.danger
        }
    }

    private func sizeString(_ bytes: UInt64) -> String {
        if bytes < 1024 { return "\(bytes) B" }
        if bytes < 1024 * 1024 { return String(format: "%.1f KB", Double(bytes) / 1024.0) }
        return String(format: "%.1f MB", Double(bytes) / (1024.0 * 1024.0))
    }
}
