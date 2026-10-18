import Foundation

/// Mirrors `hitt_core::core::auth::AuthConfig`.
///
/// Rust serde encoding (externally tagged):
///   Bearer { token }     → {"Bearer": {"token": "..."}}
///   Basic { u, p }       → {"Basic": {"username": "...", "password": "..."}}
///   ApiKey { ... }       → {"ApiKey": {...}}
///   Inherit              → "Inherit"
///   None                 → "None"
enum AuthConfig: Codable, Hashable {
    case bearer(token: String)
    case basic(username: String, password: String)
    case apiKey(key: String, value: String, location: ApiKeyLocation)
    case inherit
    case none

    enum ApiKeyLocation: String, Codable, Hashable, CaseIterable {
        case header = "Header"
        case queryParam = "QueryParam"
    }

    private enum Tag: String { case bearer = "Bearer", basic = "Basic", apiKey = "ApiKey", inherit = "Inherit", none = "None" }
    private enum WrapperKey: String, CodingKey { case Bearer, Basic, ApiKey }
    private enum BearerKeys: String, CodingKey { case token }
    private enum BasicKeys: String, CodingKey { case username, password }
    private enum ApiKeyKeys: String, CodingKey { case key, value, location }

    init(from decoder: Decoder) throws {
        // Unit variants are just strings
        if let s = try? decoder.singleValueContainer().decode(String.self) {
            switch s {
            case "Inherit": self = .inherit; return
            case "None": self = .none; return
            default:
                throw DecodingError.dataCorruptedError(
                    in: try decoder.singleValueContainer(),
                    debugDescription: "Unknown unit AuthConfig variant '\(s)'"
                )
            }
        }
        let c = try decoder.container(keyedBy: WrapperKey.self)
        if let inner = try? c.nestedContainer(keyedBy: BearerKeys.self, forKey: .Bearer) {
            self = .bearer(token: try inner.decode(String.self, forKey: .token))
            return
        }
        if let inner = try? c.nestedContainer(keyedBy: BasicKeys.self, forKey: .Basic) {
            self = .basic(
                username: try inner.decode(String.self, forKey: .username),
                password: try inner.decode(String.self, forKey: .password)
            )
            return
        }
        if let inner = try? c.nestedContainer(keyedBy: ApiKeyKeys.self, forKey: .ApiKey) {
            self = .apiKey(
                key: try inner.decode(String.self, forKey: .key),
                value: try inner.decode(String.self, forKey: .value),
                location: try inner.decode(ApiKeyLocation.self, forKey: .location)
            )
            return
        }
        throw DecodingError.dataCorruptedError(
            forKey: .Bearer, in: c, debugDescription: "Unknown AuthConfig shape"
        )
    }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .inherit:
            var c = encoder.singleValueContainer()
            try c.encode("Inherit")
        case .none:
            var c = encoder.singleValueContainer()
            try c.encode("None")
        case .bearer(let token):
            var c = encoder.container(keyedBy: WrapperKey.self)
            var inner = c.nestedContainer(keyedBy: BearerKeys.self, forKey: .Bearer)
            try inner.encode(token, forKey: .token)
        case .basic(let u, let p):
            var c = encoder.container(keyedBy: WrapperKey.self)
            var inner = c.nestedContainer(keyedBy: BasicKeys.self, forKey: .Basic)
            try inner.encode(u, forKey: .username)
            try inner.encode(p, forKey: .password)
        case .apiKey(let k, let v, let loc):
            var c = encoder.container(keyedBy: WrapperKey.self)
            var inner = c.nestedContainer(keyedBy: ApiKeyKeys.self, forKey: .ApiKey)
            try inner.encode(k, forKey: .key)
            try inner.encode(v, forKey: .value)
            try inner.encode(loc, forKey: .location)
        }
    }

    var displayName: String {
        switch self {
        case .bearer: "Bearer Token"
        case .basic: "Basic Auth"
        case .apiKey: "API Key"
        case .inherit: "Inherit"
        case .none: "No Auth"
        }
    }

    var tag: String {
        switch self {
        case .bearer: "bearer"
        case .basic: "basic"
        case .apiKey: "apiKey"
        case .inherit: "inherit"
        case .none: "none"
        }
    }
}
