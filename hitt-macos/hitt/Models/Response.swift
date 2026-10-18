import Foundation

/// Mirrors `hitt_core::core::response::ResponseBody`.
/// Rust `RequestTiming` serializes each duration as milliseconds (u64).
struct ResponseTiming: Codable, Hashable {
    var dnsLookup: UInt64
    var tcpConnect: UInt64
    var tlsHandshake: UInt64?
    var firstByte: UInt64
    var contentDownload: UInt64
    var total: UInt64

    enum CodingKeys: String, CodingKey {
        case dnsLookup = "dns_lookup"
        case tcpConnect = "tcp_connect"
        case tlsHandshake = "tls_handshake"
        case firstByte = "first_byte"
        case contentDownload = "content_download"
        case total
    }
}

struct ResponseSize: Codable, Hashable {
    var headers: UInt64
    var body: UInt64

    var total: UInt64 { headers + body }

    var formatted: String {
        let t = total
        if t < 1024 { return "\(t) B" }
        if t < 1024 * 1024 { return String(format: "%.1f KB", Double(t) / 1024.0) }
        return String(format: "%.1f MB", Double(t) / (1024.0 * 1024.0))
    }
}

struct ResponseCookie: Codable, Hashable {
    var name: String
    var value: String
    var domain: String?
    var path: String?
    var expires: String?
    var httpOnly: Bool
    var secure: Bool

    enum CodingKeys: String, CodingKey {
        case name, value, domain, path, expires
        case httpOnly = "http_only"
        case secure
    }
}

enum ResponseBody: Codable, Hashable {
    case text(String)
    case json(String)
    case xml(String)
    case html(String)
    case binary(Data)
    case empty

    private enum WrapperKey: String, CodingKey {
        case Text, Json, Xml, Html, Binary
    }

    init(from decoder: Decoder) throws {
        if let s = try? decoder.singleValueContainer().decode(String.self), s == "Empty" {
            self = .empty; return
        }
        let c = try decoder.container(keyedBy: WrapperKey.self)
        if c.contains(.Text) { self = .text(try c.decode(String.self, forKey: .Text)); return }
        if c.contains(.Json) { self = .json(try c.decode(String.self, forKey: .Json)); return }
        if c.contains(.Xml)  { self = .xml(try c.decode(String.self, forKey: .Xml));  return }
        if c.contains(.Html) { self = .html(try c.decode(String.self, forKey: .Html)); return }
        if c.contains(.Binary) {
            let bytes = try c.decode([UInt8].self, forKey: .Binary)
            self = .binary(Data(bytes)); return
        }
        throw DecodingError.dataCorruptedError(
            forKey: .Text, in: c, debugDescription: "Unknown ResponseBody variant"
        )
    }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .empty:
            var c = encoder.singleValueContainer(); try c.encode("Empty")
        case .text(let s):
            var c = encoder.container(keyedBy: WrapperKey.self); try c.encode(s, forKey: .Text)
        case .json(let s):
            var c = encoder.container(keyedBy: WrapperKey.self); try c.encode(s, forKey: .Json)
        case .xml(let s):
            var c = encoder.container(keyedBy: WrapperKey.self); try c.encode(s, forKey: .Xml)
        case .html(let s):
            var c = encoder.container(keyedBy: WrapperKey.self); try c.encode(s, forKey: .Html)
        case .binary(let d):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode([UInt8](d), forKey: .Binary)
        }
    }

    var text: String? {
        switch self {
        case .text(let s), .json(let s), .xml(let s), .html(let s): s
        case .binary(let d): "<binary \(d.count) bytes>"
        case .empty: nil
        }
    }
}

/// Mirrors `hitt_core::core::response::Response` — used for the
/// `full_json` payload returned alongside FfiResponse.
struct Response: Codable, Hashable, Identifiable {
    var id: String
    var status: UInt16
    var statusText: String
    var headers: [KeyValuePair]
    var body: ResponseBody
    var cookies: [ResponseCookie]
    var timing: ResponseTiming
    var size: ResponseSize
    var assertionResults: [AssertionResultModel]
    var timestamp: String

    enum CodingKeys: String, CodingKey {
        case id, status
        case statusText = "status_text"
        case headers, body, cookies, timing, size
        case assertionResults = "assertion_results"
        case timestamp
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        status = try c.decode(UInt16.self, forKey: .status)
        statusText = try c.decode(String.self, forKey: .statusText)
        headers = try c.decode([KeyValuePair].self, forKey: .headers)
        body = try c.decode(ResponseBody.self, forKey: .body)
        cookies = try c.decode([ResponseCookie].self, forKey: .cookies)
        timing = try c.decode(ResponseTiming.self, forKey: .timing)
        size = try c.decode(ResponseSize.self, forKey: .size)
        assertionResults =
            (try? c.decode([AssertionResultModel].self, forKey: .assertionResults)) ?? []
        timestamp = try c.decode(String.self, forKey: .timestamp)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(status, forKey: .status)
        try c.encode(statusText, forKey: .statusText)
        try c.encode(headers, forKey: .headers)
        try c.encode(body, forKey: .body)
        try c.encode(cookies, forKey: .cookies)
        try c.encode(timing, forKey: .timing)
        try c.encode(size, forKey: .size)
        try c.encode(assertionResults, forKey: .assertionResults)
        try c.encode(timestamp, forKey: .timestamp)
    }

    var statusCategory: StatusCategory {
        switch status {
        case 100..<200: .info
        case 200..<300: .success
        case 300..<400: .redirect
        case 400..<500: .clientError
        case 500..<600: .serverError
        default: .unknown
        }
    }
}

enum StatusCategory {
    case info, success, redirect, clientError, serverError, unknown
}
