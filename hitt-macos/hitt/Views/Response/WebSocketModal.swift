import SwiftUI

struct WebSocketModal: View {
    @Environment(AppViewModel.self) private var appVM
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        @Bindable var ws = appVM.websocket
        VStack(spacing: 0) {
            header
            Divider().background(HittTheme.Colors.borderSubtle)
            connectionBar(ws: ws)
            Divider().background(HittTheme.Colors.borderSubtle)
            messageStream(ws: ws)
            Divider().background(HittTheme.Colors.borderSubtle)
            sendBar(ws: ws)
        }
        .frame(minWidth: 720, minHeight: 520)
        .background(HittTheme.Colors.surface)
    }

    private var header: some View {
        HStack {
            Text("WebSocket")
                .font(HittTheme.Typography.title)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            connectionIndicator
            Spacer()
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
        let ws = appVM.websocket
        HStack(spacing: 4) {
            Circle().fill(ws.isConnected ? HittTheme.Colors.success : HittTheme.Colors.textTertiary)
                .frame(width: 7, height: 7)
            Text(ws.isConnected ? "connected" : "idle")
                .font(HittTheme.Typography.captionBold)
                .foregroundStyle(HittTheme.Colors.textSecondary)
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 2)
        .background(HittTheme.Colors.surfaceElevated.opacity(0.5))
        .clipShape(Capsule())
    }

    private func connectionBar(ws: WebSocketViewModel) -> some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            TextField("wss://…", text: Binding(
                get: { ws.url }, set: { ws.url = $0 }
            ))
            .textFieldStyle(.plain)
            .font(HittTheme.Typography.mono)
            .padding(.horizontal, HittTheme.Spacing.md)
            .frame(height: 32)
            .background(HittTheme.Colors.background.opacity(0.5))
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.medium))
            .disabled(ws.isConnected)

            if ws.isConnected {
                Button("Disconnect") { Task { await ws.disconnect() } }
                    .buttonStyle(.primary)
            } else {
                Button("Connect") { Task { await ws.connect() } }
                    .buttonStyle(.primary)
            }
        }
        .padding(HittTheme.Spacing.md)
    }

    private func messageStream(ws: WebSocketViewModel) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    if let err = ws.errorMessage {
                        Text(err)
                            .font(HittTheme.Typography.caption)
                            .foregroundStyle(HittTheme.Colors.danger)
                            .padding(HittTheme.Spacing.sm)
                    }
                    ForEach(ws.events) { row in
                        eventRow(row.event)
                            .id(row.id)
                    }
                }
                .padding(HittTheme.Spacing.sm)
            }
            .background(HittTheme.Colors.background.opacity(0.4))
            .onChange(of: ws.events.count) {
                if let last = ws.events.last {
                    withAnimation(.easeOut(duration: 0.1)) {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func eventRow(_ event: WsStreamEvent) -> some View {
        switch event {
        case .opened:
            markerRow(icon: "bolt.fill", text: "connected", color: HittTheme.Colors.success)
        case .closed:
            markerRow(icon: "power", text: "disconnected", color: HittTheme.Colors.textTertiary)
        case .error(let msg):
            markerRow(icon: "exclamationmark.triangle.fill", text: msg, color: HittTheme.Colors.danger)
        case .message(let direction, let text, let ts):
            messageRow(direction: direction, content: text, timestampMs: ts)
        case .binary(let direction, let size, let ts):
            messageRow(direction: direction, content: "<binary \(size) bytes>", timestampMs: ts)
        }
    }

    private func messageRow(direction: WsStreamEvent.Direction, content: String, timestampMs: UInt64) -> some View {
        let isOut = direction == .outgoing
        return HStack(alignment: .top, spacing: HittTheme.Spacing.sm) {
            Image(systemName: isOut ? "arrow.up.right" : "arrow.down.left")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(isOut ? HittTheme.Colors.accent : HittTheme.Colors.success)
                .frame(width: 14)
                .padding(.top, 2)
            Text(content)
                .font(HittTheme.Typography.mono)
                .foregroundStyle(HittTheme.Colors.textPrimary)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(formatTime(timestampMs))
                .font(HittTheme.Typography.monoSmall)
                .foregroundStyle(HittTheme.Colors.textTertiary)
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 4)
        .background(isOut ? HittTheme.Colors.accent.opacity(0.05) : Color.clear)
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }

    private func markerRow(icon: String, text: String, color: Color) -> some View {
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

    private func sendBar(ws: WebSocketViewModel) -> some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            TextField("Message to send…", text: Binding(
                get: { ws.outgoingMessage }, set: { ws.outgoingMessage = $0 }
            ))
            .textFieldStyle(.plain)
            .font(HittTheme.Typography.mono)
            .padding(.horizontal, HittTheme.Spacing.md)
            .frame(height: 32)
            .background(HittTheme.Colors.background.opacity(0.5))
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.medium))
            .onSubmit { Task { await ws.send() } }
            .disabled(!ws.isConnected)

            Button("Send") { Task { await ws.send() } }
                .buttonStyle(.primary)
                .disabled(!ws.isConnected || ws.outgoingMessage.isEmpty)
        }
        .padding(HittTheme.Spacing.md)
    }

    private func formatTime(_ ms: UInt64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss"
        return f.string(from: date)
    }
}
