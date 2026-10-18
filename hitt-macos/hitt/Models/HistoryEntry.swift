import Foundation

/// Mirrors `hitt_core::core::history::HistoryEntry`.
struct HistoryEntry: Codable, Hashable, Identifiable {
    var id: String
    var method: HttpMethod
    var url: String
    var status: UInt16?
    var durationMs: UInt64?
    var sizeBytes: UInt64?
    var timestamp: String
    var collectionId: String?
    var requestId: String?
    var responseBody: String?
    var requestBody: String?

    enum CodingKeys: String, CodingKey {
        case id, method, url, status
        case durationMs = "duration_ms"
        case sizeBytes = "size_bytes"
        case timestamp
        case collectionId = "collection_id"
        case requestId = "request_id"
        case responseBody = "response_body"
        case requestBody = "request_body"
    }

    var shortUrl: String {
        let stripped = url
            .replacingOccurrences(of: "https://", with: "")
            .replacingOccurrences(of: "http://", with: "")
        if stripped.count > 60 {
            return String(stripped.prefix(57)) + "…"
        }
        return stripped
    }
}
