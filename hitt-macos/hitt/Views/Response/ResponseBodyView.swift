import SwiftUI

struct ResponseBodyView: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        if let highlighted = highlightedBody {
            ScrollView([.vertical, .horizontal]) {
                Text(highlighted)
                    .font(HittTheme.Typography.mono)
                    .textSelection(.enabled)
                    .padding(HittTheme.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(HittTheme.Colors.background.opacity(0.4))
        } else if appVM.response.lastResponse != nil || appVM.response.lastFfi != nil {
            // Fallback: plain mono text for empty/binary bodies.
            ScrollView([.vertical, .horizontal]) {
                Text(appVM.response.displayBody)
                    .font(HittTheme.Typography.mono)
                    .foregroundStyle(HittTheme.Colors.textPrimary)
                    .textSelection(.enabled)
                    .padding(HittTheme.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(HittTheme.Colors.background.opacity(0.4))
        } else {
            EmptyStateView(
                icon: "paperplane",
                title: "Send a request",
                subtitle: "Cmd+⏎ to fire the current request"
            )
        }
    }

    /// Pick a highlighter based on the response body variant. Returns nil
    /// for empty / binary / unknown — the fallback branch handles those.
    private var highlightedBody: AttributedString? {
        guard let resp = appVM.response.lastResponse else {
            // FfiResponse arrived without a decoded Response — try to detect JSON.
            let raw = appVM.response.displayBody
            if looksLikeJson(raw) {
                return JsonHighlighter.highlight(appVM.response.displayBody)
            }
            return nil
        }
        switch resp.body {
        case .json:
            return JsonHighlighter.highlight(appVM.response.displayBody)
        case .xml, .html:
            return XmlHighlighter.highlight(appVM.response.displayBody)
        case .text:
            // Still try JSON heuristically (some APIs return JSON with text/plain).
            let raw = appVM.response.displayBody
            if looksLikeJson(raw) {
                return JsonHighlighter.highlight(raw)
            }
            return nil
        case .binary, .empty:
            return nil
        }
    }

    private func looksLikeJson(_ s: String) -> Bool {
        let trimmed = s.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.hasPrefix("{") || trimmed.hasPrefix("[")
    }
}
