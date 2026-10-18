import SwiftUI

struct SseModal: View {
    @Environment(AppViewModel.self) private var appVM
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        @Bindable var sse = appVM.sse
        VStack(spacing: 0) {
            header
            Divider().background(HittTheme.Colors.borderSubtle)
            connectionBar(sse: sse)
            Divider().background(HittTheme.Colors.borderSubtle)
            content(sse: sse)
        }
        .frame(minWidth: 720, minHeight: 520)
        .background(HittTheme.Colors.surface)
    }

    private var header: some View {
        @Bindable var sse = appVM.sse
        return HStack {
            Text("Server-Sent Events")
                .font(HittTheme.Typography.title)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            connectionIndicator
            Spacer()
            Toggle("Accumulate", isOn: Binding(
                get: { sse.accumulateMode },
                set: { sse.accumulateMode = $0 }
            ))
            .font(HittTheme.Typography.caption)
            .toggleStyle(.switch)
            .controlSize(.small)
            Button { dismiss() } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 16))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .buttonStyle(.plain)
        }
        .padding(HittTheme.Spacing.lg)
    }

    @ViewBuilder private var connectionIndicator: some View {
        let sse = appVM.sse
        HStack(spacing: 4) {
            Circle().fill(sse.isConnected ? HittTheme.Colors.success : HittTheme.Colors.textTertiary)
                .frame(width: 7, height: 7)
            Text(sse.isConnected ? "streaming" : "idle")
                .font(HittTheme.Typography.captionBold)
                .foregroundStyle(HittTheme.Colors.textSecondary)
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 2)
        .background(HittTheme.Colors.surfaceElevated.opacity(0.5))
        .clipShape(Capsule())
    }

    private func connectionBar(sse: SseViewModel) -> some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            TextField("https://…", text: Binding(
                get: { sse.url }, set: { sse.url = $0 }
            ))
            .textFieldStyle(.plain)
            .font(HittTheme.Typography.mono)
            .padding(.horizontal, HittTheme.Spacing.md)
            .frame(height: 32)
            .background(HittTheme.Colors.background.opacity(0.5))
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.medium))
            .disabled(sse.isConnected)

            if sse.isConnected {
                Button("Disconnect") { Task { await sse.disconnect() } }
                    .buttonStyle(.primary)
            } else {
                Button("Connect") { Task { await sse.connect() } }
                    .buttonStyle(.primary)
            }
        }
        .padding(HittTheme.Spacing.md)
    }

    @ViewBuilder
    private func content(sse: SseViewModel) -> some View {
        if sse.accumulateMode {
            ScrollView {
                Text(sse.accumulatedText.isEmpty ? "Waiting for events…" : sse.accumulatedText)
                    .font(HittTheme.Typography.mono)
                    .foregroundStyle(HittTheme.Colors.textPrimary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(HittTheme.Spacing.md)
            }
            .background(HittTheme.Colors.background.opacity(0.4))
        } else {
            eventList(sse: sse)
        }
    }

    private func eventList(sse: SseViewModel) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 4) {
                    if let err = sse.errorMessage {
                        Text(err)
                            .font(HittTheme.Typography.caption)
                            .foregroundStyle(HittTheme.Colors.danger)
                            .padding(HittTheme.Spacing.sm)
                    }
                    ForEach(sse.events) { row in
                        eventRow(row.event).id(row.id)
                    }
                }
                .padding(HittTheme.Spacing.sm)
            }
            .background(HittTheme.Colors.background.opacity(0.4))
            .onChange(of: sse.events.count) {
                if let last = sse.events.last {
                    withAnimation(.easeOut(duration: 0.1)) {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func eventRow(_ event: SseStreamEvent) -> some View {
        switch event {
        case .opened:
            marker(icon: "dot.radiowaves.up.forward", text: "streaming", color: HittTheme.Colors.success)
        case .closed:
            marker(icon: "power", text: "stream closed", color: HittTheme.Colors.textTertiary)
        case .error(let msg):
            marker(icon: "exclamationmark.triangle.fill", text: msg, color: HittTheme.Colors.danger)
        case .event(let type, let id, let data, let ts):
            sseEventRow(type: type, id: id, data: data, timestampMs: ts)
        }
    }

    private func sseEventRow(type: String?, id: String?, data: String, timestampMs: UInt64) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                if let t = type, !t.isEmpty {
                    Text(t)
                        .font(HittTheme.Typography.captionBold)
                        .foregroundStyle(HittTheme.Colors.accent)
                        .padding(.horizontal, HittTheme.Spacing.xs)
                        .padding(.vertical, 1)
                        .background(HittTheme.Colors.accent.opacity(0.15))
                        .clipShape(Capsule())
                }
                if let eid = id, !eid.isEmpty {
                    Text("#\(eid)")
                        .font(HittTheme.Typography.monoSmall)
                        .foregroundStyle(HittTheme.Colors.textTertiary)
                }
                Spacer()
                Text(formatTime(timestampMs))
                    .font(HittTheme.Typography.monoSmall)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            Text(data)
                .font(HittTheme.Typography.mono)
                .foregroundStyle(HittTheme.Colors.textPrimary)
                .textSelection(.enabled)
        }
        .padding(HittTheme.Spacing.sm)
        .background(HittTheme.Colors.surfaceElevated.opacity(0.3))
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }

    private func marker(icon: String, text: String, color: Color) -> some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            Image(systemName: icon).font(.system(size: 11)).foregroundStyle(color)
            Text(text)
                .font(HittTheme.Typography.caption)
                .foregroundStyle(color)
            Spacer()
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 2)
    }

    private func formatTime(_ ms: UInt64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss"
        return f.string(from: date)
    }
}
