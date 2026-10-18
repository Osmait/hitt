import SwiftUI

struct HistoryModal: View {
    @Environment(AppViewModel.self) private var appVM
    @Environment(\.dismiss) private var dismiss

    @State private var searchText: String = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().background(HittTheme.Colors.borderSubtle)
            searchBar
            Divider().background(HittTheme.Colors.borderSubtle)
            list
        }
        .frame(minWidth: 720, minHeight: 520)
        .background(HittTheme.Colors.surface)
        .onAppear { Task { await appVM.loadHistory() } }
    }

    private var header: some View {
        HStack {
            Text("Request History")
                .font(HittTheme.Typography.title)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            Text("\(appVM.history.count) entries")
                .font(HittTheme.Typography.caption)
                .foregroundStyle(HittTheme.Colors.textTertiary)
            Spacer()
            Button("Clear") { Task { await appVM.clearHistory() } }
                .buttonStyle(.plain)
                .foregroundStyle(HittTheme.Colors.danger)
                .disabled(appVM.history.isEmpty)
            Button { dismiss() } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 16))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .buttonStyle(.plain)
        }
        .padding(HittTheme.Spacing.lg)
    }

    private var searchBar: some View {
        SearchFieldView(text: $searchText, placeholder: "Search URL or method…")
            .padding(.horizontal, HittTheme.Spacing.md)
            .padding(.vertical, HittTheme.Spacing.sm)
    }

    private var filtered: [HistoryEntry] {
        guard !searchText.isEmpty else { return appVM.history }
        return appVM.history.filter {
            $0.url.localizedCaseInsensitiveContains(searchText) ||
            $0.method.rawValue.localizedCaseInsensitiveContains(searchText)
        }
    }

    private var list: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if filtered.isEmpty {
                    EmptyStateView(
                        icon: "clock",
                        title: appVM.history.isEmpty ? "No history yet" : "No matches",
                        subtitle: appVM.history.isEmpty ? "Sent requests appear here." : nil
                    )
                    .frame(height: 200)
                } else {
                    ForEach(filtered) { entry in
                        row(entry)
                        Divider().background(HittTheme.Colors.borderSubtle.opacity(0.3))
                    }
                }
            }
        }
    }

    private func row(_ entry: HistoryEntry) -> some View {
        Button {
            appVM.openHistoryEntry(entry)
        } label: {
            HStack(spacing: HittTheme.Spacing.md) {
                MethodBadge(method: entry.method, style: .text)
                    .frame(width: 56, alignment: .leading)
                Text(entry.shortUrl)
                    .font(HittTheme.Typography.mono)
                    .foregroundStyle(HittTheme.Colors.textPrimary)
                    .lineLimit(1)
                Spacer()
                if let status = entry.status {
                    StatusBadge(status: status, text: nil)
                }
                if let ms = entry.durationMs {
                    Text("\(ms) ms")
                        .font(HittTheme.Typography.monoSmall)
                        .foregroundStyle(durationColor(ms))
                        .frame(width: 60, alignment: .trailing)
                }
                Text(formatTimestamp(entry.timestamp))
                    .font(HittTheme.Typography.monoSmall)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
                    .frame(width: 120, alignment: .trailing)
            }
            .padding(.horizontal, HittTheme.Spacing.md)
            .padding(.vertical, HittTheme.Spacing.sm)
            .contentShape(Rectangle())
        }
        .buttonStyle(.hover)
    }

    private func durationColor(_ ms: UInt64) -> Color {
        switch ms {
        case 0..<500: HittTheme.Colors.success
        case 500..<2000: HittTheme.Colors.warning
        default: HittTheme.Colors.danger
        }
    }

    private func formatTimestamp(_ iso: String) -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = f.date(from: iso) {
            let out = DateFormatter()
            out.dateFormat = "MMM d, HH:mm:ss"
            return out.string(from: date)
        }
        return iso
    }
}
