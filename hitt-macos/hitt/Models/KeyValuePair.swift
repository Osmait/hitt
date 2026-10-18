import Foundation

/// Mirrors `hitt_core::core::request::KeyValuePair`.
struct KeyValuePair: Codable, Identifiable, Hashable {
    var id: UUID = UUID()
    var key: String = ""
    var value: String = ""
    var enabled: Bool = true
    var description: String?

    enum CodingKeys: String, CodingKey {
        case key, value, enabled, description
    }

    init(id: UUID = UUID(), key: String = "", value: String = "", enabled: Bool = true, description: String? = nil) {
        self.id = id
        self.key = key
        self.value = value
        self.enabled = enabled
        self.description = description
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = UUID()
        key = try c.decode(String.self, forKey: .key)
        value = try c.decode(String.self, forKey: .value)
        enabled = try c.decode(Bool.self, forKey: .enabled)
        description = try c.decodeIfPresent(String.self, forKey: .description)
    }
}
