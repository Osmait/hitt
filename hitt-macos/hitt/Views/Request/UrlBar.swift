import SwiftUI

/// The top bar: method picker + URL input + Send button.
struct UrlBar: View {
    @Environment(AppViewModel.self) private var appVM
    @FocusState private var urlFocused: Bool

    var body: some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            methodPicker
            urlField
            curlButton
            sendButton
        }
        .padding(.horizontal, HittTheme.Spacing.md)
        .padding(.vertical, HittTheme.Spacing.sm)
        .background(HittTheme.Colors.surface)
    }

    @ViewBuilder private var methodPicker: some View {
        @Bindable var editor = appVM.editor
        Menu {
            ForEach(HttpMethod.allCases) { m in
                Button {
                    editor.request.method = m
                } label: {
                    HStack {
                        Text(m.rawValue).monospaced()
                        Spacer()
                        if editor.request.method == m {
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Text(editor.request.method.rawValue)
                    .font(HittTheme.Typography.monoBold)
                    .foregroundStyle(editor.request.method.color)
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .padding(.horizontal, HittTheme.Spacing.md)
            .frame(height: HittTheme.Size.urlBarHeight)
            .background(HittTheme.Colors.background.opacity(0.5))
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.medium))
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    @ViewBuilder private var urlField: some View {
        @Bindable var editor = appVM.editor
        HStack {
            TextField("https://api.example.com/endpoint", text: $editor.request.url)
                .textFieldStyle(.plain)
                .font(HittTheme.Typography.mono)
                .foregroundStyle(HittTheme.Colors.textPrimary)
                .focused($urlFocused)
                .onSubmit { Task { await appVM.sendRequest() } }
        }
        .padding(.horizontal, HittTheme.Spacing.md)
        .frame(height: HittTheme.Size.urlBarHeight)
        .background(HittTheme.Colors.background.opacity(0.5))
        .overlay(
            RoundedRectangle(cornerRadius: HittTheme.Radius.medium)
                .stroke(urlFocused ? HittTheme.Colors.accent.opacity(0.5) : Color.clear, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.medium))
    }

    @ViewBuilder private var sendButton: some View {
        Button {
            Task { await appVM.sendRequest() }
        } label: {
            HStack(spacing: 6) {
                if appVM.editor.isSending {
                    ProgressView()
                        .scaleEffect(0.5)
                        .frame(width: 14, height: 14)
                } else {
                    Image(systemName: "paperplane.fill")
                        .font(.system(size: 11, weight: .semibold))
                }
                Text("Send")
                    .font(HittTheme.Typography.bodyMedium)
            }
            .frame(width: 80, height: HittTheme.Size.urlBarHeight)
        }
        .buttonStyle(.primary)
        .disabled(appVM.editor.isSending)
        .keyboardShortcut(.return, modifiers: [.command])
        .overlay(alignment: .leading) { EmptyView() }
    }

    @ViewBuilder private var curlButton: some View {
        Button {
            appVM.copyAsCurl()
        } label: {
            Image(systemName: "terminal")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(HittTheme.Colors.textSecondary)
                .frame(width: 32, height: HittTheme.Size.urlBarHeight)
                .background(HittTheme.Colors.background.opacity(0.5))
                .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.medium))
        }
        .buttonStyle(.plain)
        .help("Copy as cURL")
    }
}
