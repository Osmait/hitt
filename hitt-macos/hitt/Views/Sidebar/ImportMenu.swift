import SwiftUI
import UniformTypeIdentifiers
import AppKit

/// Import menu button — lives in the sidebar header next to "COLLECTIONS +".
struct ImportMenu: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        Menu {
            Button("Postman collection…") { pick(kind: .postmanCollection) }
            Button("Postman environment…") { pick(kind: .postmanEnvironment) }
            Button("OpenAPI (YAML/JSON)…") { pick(kind: .openApi) }
            Button("HAR…") { pick(kind: .har) }
            Button("cURL from clipboard") { pasteCurl() }
            Button(".env file…") { pick(kind: .dotenv) }
        } label: {
            Image(systemName: "square.and.arrow.down")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(HittTheme.Colors.textSecondary)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("Import")
    }

    private func pick(kind: AppViewModel.ImportKind) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false

        switch kind {
        case .postmanCollection, .postmanEnvironment, .har:
            panel.allowedContentTypes = [.json]
        case .openApi:
            panel.allowedContentTypes = [.json, .yaml]
        case .dotenv:
            panel.allowedContentTypes = [.plainText, .data]
        case .curl:
            break
        }

        guard panel.runModal() == .OK, let url = panel.url else { return }
        var name = url.deletingPathExtension().lastPathComponent
        if name.isEmpty { name = "imported" }
        Task { await appVM.importFile(at: url, kind: kind, envName: name) }
    }

    private func pasteCurl() {
        let pb = NSPasteboard.general
        guard let text = pb.string(forType: .string), !text.isEmpty else {
            appVM.showToast("Clipboard empty", isError: true)
            return
        }
        appVM.importCurlFromText(text)
    }
}

extension UTType {
    static let yaml = UTType(filenameExtension: "yaml") ?? .plainText
}
