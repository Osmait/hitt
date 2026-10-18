import Foundation

/// Mirrors `hitt_core::testing::diff::DiffResult`.
struct DiffResult: Codable, Hashable {
    var lines: [DiffLine]
    var additions: Int
    var deletions: Int
    var unchanged: Int
}

struct DiffLine: Codable, Hashable, Identifiable {
    var tag: DiffTag
    var content: String
    var oldLine: Int?
    var newLine: Int?

    var id: String {
        "\(oldLine ?? -1)-\(newLine ?? -1)-\(content.hashValue)"
    }

    enum CodingKeys: String, CodingKey {
        case tag, content
        case oldLine = "old_line"
        case newLine = "new_line"
    }
}

enum DiffTag: String, Codable, Hashable {
    case equal = "Equal"
    case insert = "Insert"
    case delete = "Delete"
}
