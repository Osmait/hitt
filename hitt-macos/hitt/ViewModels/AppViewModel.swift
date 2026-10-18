import SwiftUI
import UniformTypeIdentifiers
#if canImport(AppKit)
import AppKit
#endif

/// Root @Observable that composes sub-VMs and coordinates app-level state.
@Observable
final class AppViewModel {
    let collections = CollectionsViewModel()
    let editor = RequestEditorViewModel()
    let response = ResponseViewModel()
    let environments = EnvironmentsViewModel()
    let websocket = WebSocketViewModel()
    let sse = SseViewModel()

    var isSidebarVisible: Bool = true
    var toastMessage: String?
    var toastIsError: Bool = false

    // Modals
    var isShowingLoadTest: Bool = false
    var isShowingDiff: Bool = false
    var isShowingChainRunner: Bool = false
    var isShowingWebSocket: Bool = false
    var isShowingSse: Bool = false
    var isShowingHistory: Bool = false
    var isShowingEnvironmentEditor: Bool = false
    var editingEnvironmentId: String?

    var history: [HistoryEntry] = []

    var lastLoadTestResult: LoadTestResult?
    var isRunningLoadTest: Bool = false
    var lastDiffResult: DiffResult?
    var lastChainResult: ChainRunResult?
    var isRunningChain: Bool = false

    // Response history for diff (keeps the last N response bodies)
    private(set) var responseHistory: [(name: String, body: String)] = []
    private let responseHistoryLimit = 20

    private var service: HittService { HittService.shared }
    private var toastDismissTask: Task<Void, Never>?

    // MARK: - Lifecycle

    func onAppear() {
        Task { await initialLoad() }
    }

    private func initialLoad() async {
        do {
            try await collections.load()
        } catch {
            showError(error)
        }
        do {
            try await environments.load()
        } catch {
            showError(error)
        }
    }

    // MARK: - Send request

    func sendRequest() async {
        let request = editor.request
        guard !request.url.trimmingCharacters(in: .whitespaces).isEmpty else {
            showToast("Please enter a URL", isError: true)
            return
        }
        editor.isSending = true
        defer { editor.isSending = false }

        do {
            let (ffi, full) = try await service.sendRequest(request)
            response.apply(ffi, full)
            // Remember this body for later diffing
            let name = request.name.isEmpty ? request.url : request.name
            appendResponseToHistory(name: name, body: response.displayBody)
            if ffi.status >= 400 {
                showToast("\(ffi.status) \(ffi.statusText)", isError: false)
            }
        } catch {
            response.clear()
            showError(error)
        }
    }

    private func appendResponseToHistory(name: String, body: String) {
        responseHistory.append((name: name, body: body))
        if responseHistory.count > responseHistoryLimit {
            responseHistory.removeFirst(responseHistory.count - responseHistoryLimit)
        }
    }

    // MARK: - Load test

    func runLoadTest(total: UInt32, concurrency: UInt32, timeoutMs: UInt64 = 30000) async {
        let request = editor.request
        guard !request.url.trimmingCharacters(in: .whitespaces).isEmpty else {
            showToast("Please enter a URL", isError: true)
            return
        }
        isRunningLoadTest = true
        defer { isRunningLoadTest = false }
        do {
            let result = try await service.runLoadTest(
                request,
                total: total,
                concurrency: concurrency,
                timeoutMs: timeoutMs
            )
            lastLoadTestResult = result
            showToast("Load test done: \(result.successful)/\(result.totalRequests) OK")
        } catch {
            showError(error)
        }
    }

    // MARK: - Diff

    func runDiff(left: String, right: String) async {
        do {
            lastDiffResult = try await service.runDiff(left: left, right: right)
        } catch {
            showError(error)
        }
    }

    // MARK: - Chain runner

    func runChain(collectionId: String, chainName: String) async {
        isRunningChain = true
        defer { isRunningChain = false }
        do {
            lastChainResult = try await service.runChain(
                collectionId: collectionId,
                chainName: chainName
            )
            let steps = lastChainResult?.steps ?? []
            let ok = steps.filter { $0.status == .success }.count
            showToast("Chain done: \(ok)/\(steps.count) steps OK")
        } catch {
            showError(error)
        }
    }

    // MARK: - Environment switch

    func setEnvironment(_ name: String?) async {
        do {
            try await environments.setActive(name)
            showToast(name.map { "Environment: \($0)" } ?? "No environment")
        } catch {
            showError(error)
        }
    }

    // MARK: - Collection actions

    func createCollection(name: String) async {
        do {
            try await collections.createCollection(name: name)
            showToast("Collection '\(name)' created")
        } catch {
            showError(error)
        }
    }

    func deleteCollection(_ collection: HittCollection) async {
        do {
            try await collections.delete(collection)
            showToast("Collection '\(collection.name)' deleted")
        } catch {
            showError(error)
        }
    }

    /// Save the current editor request into a collection (Phase 2).
    func saveCurrentRequest(into collectionId: String) async {
        do {
            try await collections.saveRequest(editor.request, into: collectionId)
            showToast("Saved '\(editor.request.name)'")
        } catch {
            showError(error)
        }
    }

    // MARK: - Select request from sidebar

    func selectRequest(_ request: Request) {
        editor.load(request)
        collections.selectedRequestId = request.id
        response.clear()
    }

    // MARK: - Toast

    func showToast(_ message: String, isError: Bool = false) {
        toastIsError = isError
        toastMessage = message
        toastDismissTask?.cancel()
        toastDismissTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(isError ? 4 : 2.5))
            if toastMessage == message {
                toastMessage = nil
            }
        }
    }

    func showError(_ error: Error) {
        showToast(error.localizedDescription, isError: true)
    }

    // MARK: - History

    func loadHistory() async {
        do {
            history = try await service.getHistory()
        } catch {
            showError(error)
        }
    }

    func clearHistory() async {
        do {
            try await service.clearHistory()
            history = []
            showToast("History cleared")
        } catch {
            showError(error)
        }
    }

    /// Open the given history entry in the editor (pre-fills URL + method).
    func openHistoryEntry(_ entry: HistoryEntry) {
        editor.request = Request.new(method: entry.method, url: entry.url, name: entry.shortUrl)
        isShowingHistory = false
    }

    // MARK: - Importers

    enum ImportKind {
        case postmanCollection, postmanEnvironment, openApi, har, curl, dotenv
    }

    /// Reads the file at `url` and delegates to the appropriate parser.
    /// Auto-detected `kind` is chosen by the caller from the file extension.
    func importFile(at url: URL, kind: ImportKind, envName: String = "imported") async {
        guard let content = try? String(contentsOf: url, encoding: .utf8) else {
            showError(HittError.encoding)
            return
        }
        do {
            switch kind {
            case .postmanCollection:
                let col = try service.importPostmanCollection(content)
                try await collections.upsertImported(col)
                showToast("Imported '\(col.name)'")
            case .openApi:
                let col = try service.importOpenAPI(content)
                try await collections.upsertImported(col)
                showToast("Imported '\(col.name)'")
            case .har:
                let col = try service.importHar(content)
                try await collections.upsertImported(col)
                showToast("Imported '\(col.name)'")
            case .postmanEnvironment:
                let env = try service.importPostmanEnvironment(content)
                try await environments.save(env)
                showToast("Imported env '\(env.name)'")
            case .curl:
                let req = try service.importCurl(content)
                editor.load(req)
                showToast("cURL parsed")
            case .dotenv:
                let env = try service.importDotenv(content, envName: envName)
                try await environments.save(env)
                showToast("Imported env '\(env.name)'")
            }
        } catch {
            showError(error)
        }
    }

    /// Parse a cURL command from clipboard string (no file IO).
    func importCurlFromText(_ content: String) {
        do {
            let req = try service.importCurl(content)
            editor.load(req)
            showToast("cURL parsed")
        } catch {
            showError(error)
        }
    }

    // MARK: - Environment editor

    func openEnvironmentEditor(_ envId: String) {
        editingEnvironmentId = envId
        isShowingEnvironmentEditor = true
    }

    // MARK: - Exporters

    /// Copy the current editor request as a cURL command to the clipboard.
    func copyAsCurl() {
        do {
            let curl = try service.exportToCurl(editor.request)
            #if canImport(AppKit)
            let pb = NSPasteboard.general
            pb.clearContents()
            pb.setString(curl, forType: .string)
            #endif
            showToast("cURL copied to clipboard")
        } catch {
            showError(error)
        }
    }

    /// Export a collection as Markdown to a user-picked file.
    func exportCollectionAsMarkdown(_ collection: HittCollection) {
        do {
            let md = try service.exportToMarkdown(collection)
            save(text: md, suggestedName: "\(collection.name).md", allowedExt: "md")
        } catch {
            showError(error)
        }
    }

    /// Export a collection as Postman v2.1 JSON to a user-picked file.
    func exportCollectionAsPostman(_ collection: HittCollection) {
        do {
            let json = try service.exportPostmanCollection(collection)
            save(text: json, suggestedName: "\(collection.name).postman_collection.json", allowedExt: "json")
        } catch {
            showError(error)
        }
    }

    /// Export an environment as Postman JSON to a user-picked file.
    func exportEnvironmentAsPostman(_ env: HittEnvironment) {
        do {
            let json = try service.exportPostmanEnvironment(env)
            save(text: json, suggestedName: "\(env.name).postman_environment.json", allowedExt: "json")
        } catch {
            showError(error)
        }
    }

    private func save(text: String, suggestedName: String, allowedExt: String) {
        #if canImport(AppKit)
        let panel = NSSavePanel()
        panel.nameFieldStringValue = suggestedName
        if let ut = UTType(filenameExtension: allowedExt) {
            panel.allowedContentTypes = [ut]
        }
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try text.write(to: url, atomically: true, encoding: .utf8)
            showToast("Saved \(url.lastPathComponent)")
        } catch {
            showError(error)
        }
        #endif
    }
}
