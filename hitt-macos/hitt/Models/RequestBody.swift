import Foundation

/// Mirrors `hitt_core::core::request::RequestBody`.
enum RequestBody: Codable, Hashable {
    case json(String)
    case formData([KeyValuePair])
    case formUrlEncoded([KeyValuePair])
    case raw(content: String, contentType: String)
    case binary(path: String)
    case graphQL(query: String, variables: String?)
    case protobuf(message: String)
    case none

    private enum WrapperKey: String, CodingKey {
        case Json, FormData, FormUrlEncoded, Raw, Binary, GraphQL, Protobuf
    }
    private enum RawKeys: String, CodingKey { case content, content_type }
    private enum GqlKeys: String, CodingKey { case query, variables }
    private enum PbKeys: String, CodingKey { case message }

    init(from decoder: Decoder) throws {
        // Unit variant — plain string "None"
        if let s = try? decoder.singleValueContainer().decode(String.self), s == "None" {
            self = .none; return
        }
        let c = try decoder.container(keyedBy: WrapperKey.self)

        if c.contains(.Json) {
            self = .json(try c.decode(String.self, forKey: .Json)); return
        }
        if c.contains(.FormData) {
            self = .formData(try c.decode([KeyValuePair].self, forKey: .FormData)); return
        }
        if c.contains(.FormUrlEncoded) {
            self = .formUrlEncoded(try c.decode([KeyValuePair].self, forKey: .FormUrlEncoded)); return
        }
        if let inner = try? c.nestedContainer(keyedBy: RawKeys.self, forKey: .Raw) {
            self = .raw(
                content: try inner.decode(String.self, forKey: .content),
                contentType: try inner.decode(String.self, forKey: .content_type)
            )
            return
        }
        if c.contains(.Binary) {
            self = .binary(path: try c.decode(String.self, forKey: .Binary)); return
        }
        if let inner = try? c.nestedContainer(keyedBy: GqlKeys.self, forKey: .GraphQL) {
            self = .graphQL(
                query: try inner.decode(String.self, forKey: .query),
                variables: try inner.decodeIfPresent(String.self, forKey: .variables)
            )
            return
        }
        if let inner = try? c.nestedContainer(keyedBy: PbKeys.self, forKey: .Protobuf) {
            self = .protobuf(message: try inner.decode(String.self, forKey: .message)); return
        }
        throw DecodingError.dataCorruptedError(
            forKey: .Json, in: c, debugDescription: "Unknown RequestBody variant"
        )
    }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .none:
            var c = encoder.singleValueContainer()
            try c.encode("None")
        case .json(let s):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(s, forKey: .Json)
        case .formData(let items):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(items, forKey: .FormData)
        case .formUrlEncoded(let items):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(items, forKey: .FormUrlEncoded)
        case .raw(let content, let ct):
            var c = encoder.container(keyedBy: WrapperKey.self)
            var inner = c.nestedContainer(keyedBy: RawKeys.self, forKey: .Raw)
            try inner.encode(content, forKey: .content)
            try inner.encode(ct, forKey: .content_type)
        case .binary(let path):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(path, forKey: .Binary)
        case .graphQL(let q, let v):
            var c = encoder.container(keyedBy: WrapperKey.self)
            var inner = c.nestedContainer(keyedBy: GqlKeys.self, forKey: .GraphQL)
            try inner.encode(q, forKey: .query)
            try inner.encodeIfPresent(v, forKey: .variables)
        case .protobuf(let m):
            var c = encoder.container(keyedBy: WrapperKey.self)
            var inner = c.nestedContainer(keyedBy: PbKeys.self, forKey: .Protobuf)
            try inner.encode(m, forKey: .message)
        }
    }

    var tag: String {
        switch self {
        case .json: "json"
        case .formData: "formData"
        case .formUrlEncoded: "formUrlEncoded"
        case .raw: "raw"
        case .binary: "binary"
        case .graphQL: "graphQL"
        case .protobuf: "protobuf"
        case .none: "none"
        }
    }

    var displayName: String {
        switch self {
        case .json: "JSON"
        case .formData: "form-data"
        case .formUrlEncoded: "x-www-form-urlencoded"
        case .raw: "raw"
        case .binary: "binary"
        case .graphQL: "GraphQL"
        case .protobuf: "protobuf"
        case .none: "none"
        }
    }
}
