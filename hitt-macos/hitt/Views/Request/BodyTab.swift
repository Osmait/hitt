import SwiftUI

struct BodyTab: View {
    @Environment(AppViewModel.self) private var appVM

    private enum Kind: String, CaseIterable, Identifiable {
        case none = "None"
        case json = "JSON"
        case raw = "Raw"
        case formUrl = "x-www-form-urlencoded"
        var id: String { rawValue }
    }

    @State private var rawText: String = ""

    var body: some View {
        @Bindable var editor = appVM.editor
        VStack(alignment: .leading, spacing: 0) {
            // Type picker
            HStack(spacing: HittTheme.Spacing.xs) {
                ForEach(Kind.allCases) { k in
                    Button {
                        apply(kind: k, editor: editor)
                    } label: {
                        Text(k.rawValue)
                            .font(HittTheme.Typography.caption)
                            .foregroundStyle(currentKind == k ? HittTheme.Colors.textPrimary : HittTheme.Colors.textTertiary)
                            .padding(.horizontal, HittTheme.Spacing.sm)
                            .padding(.vertical, 3)
                            .background(currentKind == k ? HittTheme.Colors.surfaceElevated : Color.clear)
                            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
                    }
                    .buttonStyle(.hover)
                }
                Spacer()
            }
            .padding(HittTheme.Spacing.sm)

            Divider().background(HittTheme.Colors.borderSubtle)

            switch currentKind {
            case .none:
                EmptyStateView(icon: "doc", title: "No body", subtitle: "Pick JSON, Raw, or form-encoded above.")
            case .json:
                bodyEditor(prompt: "{\n  \"key\": \"value\"\n}", textBinding: Binding(
                    get: { rawText },
                    set: { rawText = $0; editor.request.body = .json($0) }
                ))
            case .raw:
                bodyEditor(prompt: "raw body…", textBinding: Binding(
                    get: { rawText },
                    set: { rawText = $0; editor.request.body = .raw(content: $0, contentType: "text/plain") }
                ))
            case .formUrl:
                if case .formUrlEncoded(let items) = editor.request.body {
                    KeyValueTable(items: Binding(
                        get: { items },
                        set: { editor.request.body = .formUrlEncoded($0) }
                    ), keyPlaceholder: "Field", valuePlaceholder: "Value")
                } else {
                    Color.clear.onAppear { editor.request.body = .formUrlEncoded([]) }
                }
            }
        }
        .onAppear {
            if case .json(let s) = editor.request.body { rawText = s }
            if case .raw(let s, _) = editor.request.body { rawText = s }
        }
    }

    private var currentKind: Kind {
        switch appVM.editor.request.body {
        case .none, Optional.none: .none
        case .some(.json): .json
        case .some(.raw): .raw
        case .some(.formUrlEncoded): .formUrl
        default: .none
        }
    }

    private func apply(kind: Kind, editor: RequestEditorViewModel) {
        switch kind {
        case .none: editor.request.body = Optional<RequestBody>.none
        case .json: editor.request.body = .json(rawText)
        case .raw: editor.request.body = .raw(content: rawText, contentType: "text/plain")
        case .formUrl: editor.request.body = .formUrlEncoded([])
        }
    }

    private func bodyEditor(prompt: String, textBinding: Binding<String>) -> some View {
        TextEditor(text: textBinding)
            .font(HittTheme.Typography.mono)
            .foregroundStyle(HittTheme.Colors.textPrimary)
            .scrollContentBackground(.hidden)
            .background(HittTheme.Colors.background.opacity(0.4))
            .padding(HittTheme.Spacing.sm)
    }
}
