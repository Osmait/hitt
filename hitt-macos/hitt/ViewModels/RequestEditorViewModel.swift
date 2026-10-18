import Foundation

/// State for the currently-edited request (URL, method, headers, body, auth).
@Observable
final class RequestEditorViewModel {
    var request: Request = Request.new()
    var selectedTab: EditorTab = .headers
    var isSending: Bool = false

    enum EditorTab: String, CaseIterable, Identifiable {
        case params = "Params"
        case headers = "Headers"
        case body = "Body"
        case auth = "Auth"
        case tests = "Tests"
        var id: String { rawValue }
    }

    /// Compute how many enabled items each tab has (for badge counts).
    var paramsCount: Int { request.params.filter(\.enabled).count }
    var headersCount: Int { request.headers.filter(\.enabled).count }
    var assertionsCount: Int { request.assertions.filter(\.enabled).count }

    var authTag: String { request.auth?.tag ?? "none" }
    var bodyTag: String { request.body?.tag ?? "none" }

    /// Load a request into the editor (e.g. when the user taps one in the sidebar).
    func load(_ request: Request) {
        self.request = request
    }

    /// Reset to an empty request, preserving only the URL (optional).
    func reset(preservingUrl: Bool = false) {
        let url = preservingUrl ? request.url : ""
        request = Request.new(url: url)
    }
}
