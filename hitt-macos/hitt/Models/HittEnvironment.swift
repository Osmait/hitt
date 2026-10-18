import Foundation

/// Mirrors `hitt_core::core::environment::EnvironmentVariable`.
struct EnvironmentVariable: Codable, Identifiable, Hashable {
    var id = UUID()
    var key: String
    var value: String
    var initialValue: String
    var enabled: Bool
    var secret: Bool

    enum CodingKeys: String, CodingKey {
        case key, value
        case initialValue = "initial_value"
        case enabled, secret
    }

    init(key: String = "", value: String = "", enabled: Bool = true, secret: Bool = false) {
        self.key = key
        self.value = value
        self.initialValue = value
        self.enabled = enabled
        self.secret = secret
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = UUID()
        key = try c.decode(String.self, forKey: .key)
        value = try c.decode(String.self, forKey: .value)
        initialValue = try c.decode(String.self, forKey: .initialValue)
        enabled = try c.decode(Bool.self, forKey: .enabled)
        secret = try c.decode(Bool.self, forKey: .secret)
    }
}

/// Mirrors `hitt_core::core::environment::Environment`.
///
/// Renamed `HittEnvironment` to avoid collision with Foundation's
/// `ProcessInfo.Environment` and SwiftUI's `Environment` property wrapper.
struct HittEnvironment: Codable, Identifiable, Hashable {
    var id: String
    var name: String
    var values: [EnvironmentVariable]

    static func new(name: String) -> HittEnvironment {
        HittEnvironment(id: UUID().uuidString.lowercased(), name: name, values: [])
    }
}
