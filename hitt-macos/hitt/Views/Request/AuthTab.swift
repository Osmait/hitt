import SwiftUI

struct AuthTab: View {
    @Environment(AppViewModel.self) private var appVM

    private enum Kind: String, CaseIterable, Identifiable {
        case none = "None"
        case bearer = "Bearer"
        case basic = "Basic"
        case apiKey = "API Key"
        var id: String { rawValue }
    }

    @State private var bearerToken: String = ""
    @State private var basicUser: String = ""
    @State private var basicPass: String = ""
    @State private var apiKeyKey: String = ""
    @State private var apiKeyValue: String = ""
    @State private var apiKeyLocation: AuthConfig.ApiKeyLocation = .header

    var body: some View {
        @Bindable var editor = appVM.editor
        VStack(alignment: .leading, spacing: HittTheme.Spacing.md) {
            // Kind picker
            HStack(spacing: HittTheme.Spacing.xs) {
                ForEach(Kind.allCases) { k in
                    Button {
                        select(k, editor: editor)
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

            Divider().background(HittTheme.Colors.borderSubtle)

            switch currentKind {
            case .none:
                EmptyStateView(icon: "lock.open", title: "No authentication")
            case .bearer:
                bearerFields(editor: editor)
            case .basic:
                basicFields(editor: editor)
            case .apiKey:
                apiKeyFields(editor: editor)
            }

            Spacer()
        }
        .padding(HittTheme.Spacing.md)
        .onAppear(perform: loadCurrentState)
    }

    private var currentKind: Kind {
        switch appVM.editor.request.auth {
        case nil, .some(.none), .some(.inherit): .none
        case .some(.bearer): .bearer
        case .some(.basic): .basic
        case .some(.apiKey): .apiKey
        }
    }

    @ViewBuilder
    private func bearerFields(editor: RequestEditorViewModel) -> some View {
        labeledField(
            "Token",
            text: Binding(
                get: { bearerToken },
                set: { bearerToken = $0; editor.request.auth = .bearer(token: $0) }
            ),
            isSecure: true
        )
    }

    @ViewBuilder
    private func basicFields(editor: RequestEditorViewModel) -> some View {
        labeledField(
            "Username",
            text: Binding(
                get: { basicUser },
                set: { basicUser = $0; editor.request.auth = .basic(username: $0, password: basicPass) }
            )
        )
        labeledField(
            "Password",
            text: Binding(
                get: { basicPass },
                set: { basicPass = $0; editor.request.auth = .basic(username: basicUser, password: $0) }
            ),
            isSecure: true
        )
    }

    @ViewBuilder
    private func apiKeyFields(editor: RequestEditorViewModel) -> some View {
        labeledField(
            "Key",
            text: Binding(
                get: { apiKeyKey },
                set: { apiKeyKey = $0; apply(editor) }
            )
        )
        labeledField(
            "Value",
            text: Binding(
                get: { apiKeyValue },
                set: { apiKeyValue = $0; apply(editor) }
            ),
            isSecure: true
        )
        HStack(spacing: HittTheme.Spacing.md) {
            Text("Location")
                .font(HittTheme.Typography.caption)
                .foregroundStyle(HittTheme.Colors.textSecondary)
                .frame(width: 80, alignment: .leading)
            Picker("", selection: Binding(
                get: { apiKeyLocation },
                set: { apiKeyLocation = $0; apply(editor) }
            )) {
                Text("Header").tag(AuthConfig.ApiKeyLocation.header)
                Text("Query param").tag(AuthConfig.ApiKeyLocation.queryParam)
            }
            .labelsHidden()
            .frame(width: 140)
        }
    }

    private func apply(_ editor: RequestEditorViewModel) {
        editor.request.auth = .apiKey(key: apiKeyKey, value: apiKeyValue, location: apiKeyLocation)
    }

    private func select(_ kind: Kind, editor: RequestEditorViewModel) {
        switch kind {
        case .none: editor.request.auth = Optional<AuthConfig>.none
        case .bearer: editor.request.auth = .bearer(token: bearerToken)
        case .basic: editor.request.auth = .basic(username: basicUser, password: basicPass)
        case .apiKey: editor.request.auth = .apiKey(key: apiKeyKey, value: apiKeyValue, location: apiKeyLocation)
        }
    }

    private func loadCurrentState() {
        switch appVM.editor.request.auth {
        case .some(.bearer(let t)):
            bearerToken = t
        case .some(.basic(let u, let p)):
            basicUser = u; basicPass = p
        case .some(.apiKey(let k, let v, let l)):
            apiKeyKey = k; apiKeyValue = v; apiKeyLocation = l
        default: break
        }
    }

    @ViewBuilder
    private func labeledField(_ label: String, text: Binding<String>, isSecure: Bool = false) -> some View {
        HStack(spacing: HittTheme.Spacing.md) {
            Text(label)
                .font(HittTheme.Typography.caption)
                .foregroundStyle(HittTheme.Colors.textSecondary)
                .frame(width: 80, alignment: .leading)
            Group {
                if isSecure {
                    SecureField("", text: text).textFieldStyle(.roundedBorder)
                } else {
                    TextField("", text: text).textFieldStyle(.roundedBorder)
                }
            }
            .font(HittTheme.Typography.mono)
        }
    }
}
