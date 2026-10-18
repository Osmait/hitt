import Foundation

/// Mirrors `hitt_core::core::request::Protocol`.
enum RequestProtocol: Codable, Hashable {
    case http
    case webSocket
    case sse
    case grpc(protoFile: String, service: String, method: String)

    private enum WrapperKey: String, CodingKey { case Grpc }
    private enum GrpcKeys: String, CodingKey { case proto_file, service, method }

    init(from decoder: Decoder) throws {
        if let s = try? decoder.singleValueContainer().decode(String.self) {
            switch s {
            case "Http": self = .http; return
            case "WebSocket": self = .webSocket; return
            case "Sse": self = .sse; return
            default:
                throw DecodingError.dataCorruptedError(
                    in: try decoder.singleValueContainer(),
                    debugDescription: "Unknown Protocol '\(s)'"
                )
            }
        }
        let c = try decoder.container(keyedBy: WrapperKey.self)
        let inner = try c.nestedContainer(keyedBy: GrpcKeys.self, forKey: .Grpc)
        self = .grpc(
            protoFile: try inner.decode(String.self, forKey: .proto_file),
            service: try inner.decode(String.self, forKey: .service),
            method: try inner.decode(String.self, forKey: .method)
        )
    }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .http:
            var c = encoder.singleValueContainer(); try c.encode("Http")
        case .webSocket:
            var c = encoder.singleValueContainer(); try c.encode("WebSocket")
        case .sse:
            var c = encoder.singleValueContainer(); try c.encode("Sse")
        case .grpc(let p, let s, let m):
            var c = encoder.container(keyedBy: WrapperKey.self)
            var inner = c.nestedContainer(keyedBy: GrpcKeys.self, forKey: .Grpc)
            try inner.encode(p, forKey: .proto_file)
            try inner.encode(s, forKey: .service)
            try inner.encode(m, forKey: .method)
        }
    }
}

/// Mirrors `hitt_core::core::request::Request`.
///
/// Assertions and scripts are preserved as opaque JSON values in Phase 1
/// so Swift doesn't have to mirror their full shape yet.
struct Request: Codable, Identifiable, Hashable {
    var id: String
    var name: String
    var `protocol`: RequestProtocol
    var method: HttpMethod
    var url: String
    var headers: [KeyValuePair]
    var params: [KeyValuePair]
    var auth: AuthConfig?
    var body: RequestBody?
    var assertions: [AssertionModel]
    var preRequestScript: String?
    var testScript: String?
    var description: String?
    var createdAt: String
    var updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name
        case `protocol` = "protocol"
        case method, url, headers, params, auth, body, assertions
        case preRequestScript = "pre_request_script"
        case testScript = "test_script"
        case description
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    static func new(method: HttpMethod = .GET, url: String = "", name: String = "Untitled") -> Request {
        let now = ISO8601DateFormatter.full.string(from: Date())
        return Request(
            id: UUID().uuidString.lowercased(),
            name: name,
            protocol: .http,
            method: method,
            url: url,
            headers: [],
            params: [],
            auth: nil,
            body: nil,
            assertions: [],
            preRequestScript: nil,
            testScript: nil,
            description: nil,
            createdAt: now,
            updatedAt: now
        )
    }
}

extension ISO8601DateFormatter {
    static let full: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}

/// Type-erased JSON value for fields Swift doesn't need to introspect yet.
enum JSONValue: Codable, Hashable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null; return }
        if let b = try? c.decode(Bool.self) { self = .bool(b); return }
        if let n = try? c.decode(Double.self) { self = .number(n); return }
        if let s = try? c.decode(String.self) { self = .string(s); return }
        if let a = try? c.decode([JSONValue].self) { self = .array(a); return }
        if let o = try? c.decode([String: JSONValue].self) { self = .object(o); return }
        throw DecodingError.dataCorruptedError(
            in: c, debugDescription: "Unknown JSON value"
        )
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let b): try c.encode(b)
        case .number(let n): try c.encode(n)
        case .string(let s): try c.encode(s)
        case .array(let a): try c.encode(a)
        case .object(let o): try c.encode(o)
        }
    }
}
