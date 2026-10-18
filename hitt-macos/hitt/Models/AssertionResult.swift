import Foundation

/// Mirrors `hitt_core::testing::assertion_engine::AssertionResult`.
struct AssertionResultModel: Codable, Hashable, Identifiable {
    var assertion: AssertionModel
    var passed: Bool
    var actualValue: String?
    var message: String

    var id: String { assertion.id }

    enum CodingKeys: String, CodingKey {
        case assertion, passed
        case actualValue = "actual_value"
        case message
    }
}

/// Mirrors `hitt_core::testing::assertion_engine::Assertion` with a
/// simplified view of `AssertionKind` — we only keep the display label
/// for now. Full roundtrip editing comes later.
struct AssertionModel: Codable, Hashable, Identifiable {
    var id: String
    var kind: AssertionKindModel
    var enabled: Bool
}

/// JSON-faithful mirror of `AssertionKind`. Only the variants the Swift
/// editor will create are given first-class cases; the rest survive as
/// raw JSON for round-tripping.
enum AssertionKindModel: Codable, Hashable {
    case statusEquals(UInt16)
    case statusRange(UInt16, UInt16)
    case bodyContains(String)
    case headerExists(String)
    case responseTimeLessThan(UInt64)
    case sizeLessThan(UInt64)
    /// Any variant we don't model explicitly round-trips as raw JSON.
    case other(JSONValue)

    private enum WrapperKey: String, CodingKey {
        case StatusEquals, StatusRange, BodyContains, HeaderExists
        case ResponseTimeLessThan, SizeLessThan
    }

    init(from decoder: Decoder) throws {
        // Try keyed container first (all current variants are tagged)
        if let c = try? decoder.container(keyedBy: WrapperKey.self) {
            if c.contains(.StatusEquals) {
                self = .statusEquals(try c.decode(UInt16.self, forKey: .StatusEquals)); return
            }
            if c.contains(.StatusRange) {
                let arr = try c.decode([UInt16].self, forKey: .StatusRange)
                if arr.count == 2 { self = .statusRange(arr[0], arr[1]); return }
            }
            if c.contains(.BodyContains) {
                self = .bodyContains(try c.decode(String.self, forKey: .BodyContains)); return
            }
            if c.contains(.HeaderExists) {
                self = .headerExists(try c.decode(String.self, forKey: .HeaderExists)); return
            }
            if c.contains(.ResponseTimeLessThan) {
                // Rust serializes Duration as struct {secs,nanos}; accept either ms u64 or struct.
                if let ms = try? c.decode(UInt64.self, forKey: .ResponseTimeLessThan) {
                    self = .responseTimeLessThan(ms); return
                }
                let raw = try c.decode(JSONValue.self, forKey: .ResponseTimeLessThan)
                if case .object(let o) = raw, case .number(let s) = o["secs"] ?? .null {
                    self = .responseTimeLessThan(UInt64(s * 1000)); return
                }
                self = .other(raw); return
            }
            if c.contains(.SizeLessThan) {
                self = .sizeLessThan(try c.decode(UInt64.self, forKey: .SizeLessThan)); return
            }
        }
        // Unknown variant — keep as raw JSON.
        let raw = try JSONValue(from: decoder)
        self = .other(raw)
    }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .statusEquals(let s):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(s, forKey: .StatusEquals)
        case .statusRange(let a, let b):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode([a, b], forKey: .StatusRange)
        case .bodyContains(let s):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(s, forKey: .BodyContains)
        case .headerExists(let s):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(s, forKey: .HeaderExists)
        case .responseTimeLessThan(let ms):
            // Rust Duration serde-serializes as {secs,nanos}; match that shape.
            var c = encoder.container(keyedBy: WrapperKey.self)
            let secs = ms / 1000
            let nanos = (ms % 1000) * 1_000_000
            let dur: [String: UInt64] = ["secs": secs, "nanos": nanos]
            try c.encode(dur, forKey: .ResponseTimeLessThan)
        case .sizeLessThan(let n):
            var c = encoder.container(keyedBy: WrapperKey.self)
            try c.encode(n, forKey: .SizeLessThan)
        case .other(let raw):
            try raw.encode(to: encoder)
        }
    }

    var displayLabel: String {
        switch self {
        case .statusEquals(let s): "status == \(s)"
        case .statusRange(let a, let b): "status in \(a)..\(b)"
        case .bodyContains(let s): "body contains \"\(s)\""
        case .headerExists(let h): "header \"\(h)\" exists"
        case .responseTimeLessThan(let ms): "time < \(ms) ms"
        case .sizeLessThan(let n): "size < \(n) bytes"
        case .other: "custom"
        }
    }
}
