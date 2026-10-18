import SwiftUI

struct ResponseView: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        VStack(spacing: 0) {
            ResponseToolbar()
            Divider().background(HittTheme.Colors.borderSubtle)
            content
        }
    }

    @ViewBuilder private var content: some View {
        switch appVM.response.selectedTab {
        case .body: ResponseBodyView()
        case .headers: ResponseHeadersView()
        case .cookies: cookiesList
        case .timing: timingView
        case .tests: ResponseTestsView()
        }
    }

    @ViewBuilder private var cookiesList: some View {
        if let resp = appVM.response.lastResponse, !resp.cookies.isEmpty {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: HittTheme.Spacing.sm) {
                    ForEach(resp.cookies, id: \.name) { c in
                        VStack(alignment: .leading, spacing: 2) {
                            HStack {
                                Text(c.name)
                                    .font(HittTheme.Typography.monoBold)
                                    .foregroundStyle(HittTheme.Colors.accent)
                                if c.secure {
                                    Text("secure").font(HittTheme.Typography.tiny)
                                        .padding(.horizontal, 4).padding(.vertical, 1)
                                        .background(HittTheme.Colors.success.opacity(0.15))
                                        .foregroundStyle(HittTheme.Colors.success)
                                        .clipShape(Capsule())
                                }
                                if c.httpOnly {
                                    Text("http-only").font(HittTheme.Typography.tiny)
                                        .padding(.horizontal, 4).padding(.vertical, 1)
                                        .background(HittTheme.Colors.warning.opacity(0.15))
                                        .foregroundStyle(HittTheme.Colors.warning)
                                        .clipShape(Capsule())
                                }
                            }
                            Text(c.value).font(HittTheme.Typography.mono)
                                .foregroundStyle(HittTheme.Colors.textPrimary)
                                .textSelection(.enabled)
                        }
                        .padding(HittTheme.Spacing.sm)
                        .background(HittTheme.Colors.surfaceElevated.opacity(0.5))
                        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
                    }
                }
                .padding(HittTheme.Spacing.md)
            }
        } else {
            EmptyStateView(icon: "birthday.cake", title: "No cookies")
        }
    }

    @ViewBuilder private var timingView: some View {
        if let t = appVM.response.lastResponse?.timing {
            VStack(alignment: .leading, spacing: HittTheme.Spacing.sm) {
                timingRow("DNS lookup", ms: t.dnsLookup)
                timingRow("TCP connect", ms: t.tcpConnect)
                if let tls = t.tlsHandshake {
                    timingRow("TLS handshake", ms: tls)
                }
                timingRow("First byte (TTFB)", ms: t.firstByte)
                timingRow("Content download", ms: t.contentDownload)
                Divider().background(HittTheme.Colors.borderSubtle).padding(.vertical, 4)
                timingRow("Total", ms: t.total, highlighted: true)
            }
            .padding(HittTheme.Spacing.md)
        } else {
            EmptyStateView(icon: "stopwatch", title: "No timing data")
        }
    }

    private func timingRow(_ label: String, ms: UInt64, highlighted: Bool = false) -> some View {
        HStack {
            Text(label)
                .font(HittTheme.Typography.body)
                .foregroundStyle(highlighted ? HittTheme.Colors.textPrimary : HittTheme.Colors.textSecondary)
            Spacer()
            Text("\(ms) ms")
                .font(HittTheme.Typography.monoBold)
                .foregroundStyle(highlighted ? HittTheme.Colors.accent : HittTheme.Colors.textPrimary)
        }
    }
}
