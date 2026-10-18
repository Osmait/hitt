import Foundation

/// Mirrors `hitt_core::core::collection::CollectionItem` (folders + requests).
/// For Phase 1 we expose a flat list via `allRequests`; folder nesting
/// round-trips unchanged so nothing is lost.
indirect enum CollectionItem: Codable, Hashable, Identifiable {
    case request(Request)
    case folder(id: String, name: String, items: [CollectionItem], auth: AuthConfig?, description: String?)

    var id: String {
        switch self {
        case .request(let r): r.id
        case .folder(let id, _, _, _, _): id
        }
    }

    var name: String {
        switch self {
        case .request(let r): r.name
        case .folder(_, let n, _, _, _): n
        }
    }

    private enum WrapperKey: String, CodingKey { case Request, Folder }
    private enum FolderKeys: String, CodingKey { case id, name, items, auth, description }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: WrapperKey.self)
        if c.contains(.Request) {
            let req = try c.decode(Request.self, forKey: .Request)
            self = .request(req)
            return
        }
        if c.contains(.Folder) {
            let inner = try c.nestedContainer(keyedBy: FolderKeys.self, forKey: .Folder)
            let id = try inner.decode(String.self, forKey: .id)
            let name = try inner.decode(String.self, forKey: .name)
            let items = try inner.decode([CollectionItem].self, forKey: .items)
            let auth = try inner.decodeIfPresent(AuthConfig.self, forKey: .auth)
            let desc = try inner.decodeIfPresent(String.self, forKey: .description)
            self = .folder(id: id, name: name, items: items, auth: auth, description: desc)
            return
        }
        throw DecodingError.dataCorruptedError(
            forKey: .Request, in: c, debugDescription: "Unknown CollectionItem variant"
        )
    }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .request(let r):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(r, forKey: .Request)
        case .folder(let id, let name, let items, let auth, let description):
            var c = encoder.container(keyedBy: WrapperKey.self)
            var inner = c.nestedContainer(keyedBy: FolderKeys.self, forKey: .Folder)
            try inner.encode(id, forKey: .id)
            try inner.encode(name, forKey: .name)
            try inner.encode(items, forKey: .items)
            try inner.encodeIfPresent(auth, forKey: .auth)
            try inner.encodeIfPresent(description, forKey: .description)
        }
    }
}

/// Mirrors `hitt_core::core::collection::Collection`. Phase 1 ignores
/// `cookie_jar` and `chains` contents but round-trips them via JSONValue.
struct HittCollection: Codable, Hashable, Identifiable {
    var id: String
    var name: String
    var description: String?
    var items: [CollectionItem]
    var variables: [KeyValuePair]
    var auth: AuthConfig?
    var cookieJar: JSONValue = .object([:])
    var chains: [JSONValue] = []

    enum CodingKeys: String, CodingKey {
        case id, name, description, items, variables, auth
        case cookieJar = "cookie_jar"
        case chains
    }

    static func new(name: String) -> HittCollection {
        HittCollection(
            id: UUID().uuidString.lowercased(),
            name: name,
            description: nil,
            items: [],
            variables: [],
            auth: nil,
            cookieJar: .object([:]),
            chains: []
        )
    }

    /// Flatten folder tree into a list of requests (Phase 1 use).
    var allRequests: [Request] {
        Self.flatten(items)
    }

    private static func flatten(_ items: [CollectionItem]) -> [Request] {
        var out: [Request] = []
        for item in items {
            switch item {
            case .request(let r): out.append(r)
            case .folder(_, _, let sub, _, _): out.append(contentsOf: flatten(sub))
            }
        }
        return out
    }
}
