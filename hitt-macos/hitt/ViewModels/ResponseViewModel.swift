import Foundation

/// Holds the last response and computes body display strings.
@Observable
final class ResponseViewModel {
    var lastResponse: Response?
    var lastFfi: FfiResponse?
    var selectedTab: ResponseTab = .body

    enum ResponseTab: String, CaseIterable, Identifiable {
        case body = "Body"
        case headers = "Headers"
        case cookies = "Cookies"
        case timing = "Timing"
        case tests = "Tests"
        var id: String { rawValue }
    }

    var assertionsPassed: Int { lastResponse?.assertionResults.filter(\.passed).count ?? 0 }
    var assertionsTotal: Int { lastResponse?.assertionResults.count ?? 0 }

    func apply(_ ffi: FfiResponse, _ full: Response) {
        self.lastFfi = ffi
        self.lastResponse = full
    }

    func clear() {
        lastFfi = nil
        lastResponse = nil
    }

    /// Pretty-print JSON if the content-type suggests it; otherwise return raw text.
    var displayBody: String {
        guard let resp = lastResponse else { return lastFfi?.body ?? "" }
        switch resp.body {
        case .json(let s):
            return prettyJson(s) ?? s
        case .text(let s), .xml(let s), .html(let s):
            return s
        case .binary(let d):
            return "<binary \(d.count) bytes>"
        case .empty:
            return ""
        }
    }

    var statusText: String {
        guard let ffi = lastFfi else { return "" }
        return "\(ffi.status) \(ffi.statusText)"
    }

    private func prettyJson(_ raw: String) -> String? {
        guard let data = raw.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]),
              let pretty = try? JSONSerialization.data(withJSONObject: obj, options: [.prettyPrinted, .sortedKeys]),
              let s = String(data: pretty, encoding: .utf8)
        else { return nil }
        return s
    }
}
