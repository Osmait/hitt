import Foundation

/// Mirrors `hitt-ffi::streams::WsFfiEvent` (internally tagged with `kind`).
enum WsStreamEvent: Codable, Hashable {
    case opened
    case message(direction: Direction, text: String, timestampMs: UInt64)
    case binary(direction: Direction, size: Int, timestampMs: UInt64)
    case error(message: String)
    case closed

    enum Direction: String, Codable, Hashable {
        case incoming = "in"
        case outgoing = "out"
    }

    enum CodingKeys: String, CodingKey {
        case kind, direction, text, size, message
        case timestampMs = "timestamp_ms"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "Opened": self = .opened
        case "Closed": self = .closed
        case "Error":
            self = .error(message: try c.decode(String.self, forKey: .message))
        case "Message":
            self = .message(
                direction: try c.decode(Direction.self, forKey: .direction),
                text: try c.decode(String.self, forKey: .text),
                timestampMs: try c.decode(UInt64.self, forKey: .timestampMs)
            )
        case "Binary":
            self = .binary(
                direction: try c.decode(Direction.self, forKey: .direction),
                size: try c.decode(Int.self, forKey: .size),
                timestampMs: try c.decode(UInt64.self, forKey: .timestampMs)
            )
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .kind, in: c, debugDescription: "Unknown WsStreamEvent: \(kind)"
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .opened: try c.encode("Opened", forKey: .kind)
        case .closed: try c.encode("Closed", forKey: .kind)
        case .error(let m):
            try c.encode("Error", forKey: .kind)
            try c.encode(m, forKey: .message)
        case .message(let d, let t, let ts):
            try c.encode("Message", forKey: .kind)
            try c.encode(d, forKey: .direction)
            try c.encode(t, forKey: .text)
            try c.encode(ts, forKey: .timestampMs)
        case .binary(let d, let s, let ts):
            try c.encode("Binary", forKey: .kind)
            try c.encode(d, forKey: .direction)
            try c.encode(s, forKey: .size)
            try c.encode(ts, forKey: .timestampMs)
        }
    }
}

/// Mirrors `hitt-ffi::streams::SseFfiEvent`.
enum SseStreamEvent: Codable, Hashable {
    case opened
    case event(eventType: String?, id: String?, data: String, timestampMs: UInt64)
    case error(message: String)
    case closed

    enum CodingKeys: String, CodingKey {
        case kind, message, data
        case eventType = "event_type"
        case eventId = "id"
        case timestampMs = "timestamp_ms"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "Opened": self = .opened
        case "Closed": self = .closed
        case "Error":
            self = .error(message: try c.decode(String.self, forKey: .message))
        case "Event":
            self = .event(
                eventType: try c.decodeIfPresent(String.self, forKey: .eventType),
                id: try c.decodeIfPresent(String.self, forKey: .eventId),
                data: try c.decode(String.self, forKey: .data),
                timestampMs: try c.decode(UInt64.self, forKey: .timestampMs)
            )
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .kind, in: c, debugDescription: "Unknown SseStreamEvent: \(kind)"
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .opened: try c.encode("Opened", forKey: .kind)
        case .closed: try c.encode("Closed", forKey: .kind)
        case .error(let m):
            try c.encode("Error", forKey: .kind)
            try c.encode(m, forKey: .message)
        case .event(let t, let id, let data, let ts):
            try c.encode("Event", forKey: .kind)
            try c.encodeIfPresent(t, forKey: .eventType)
            try c.encodeIfPresent(id, forKey: .eventId)
            try c.encode(data, forKey: .data)
            try c.encode(ts, forKey: .timestampMs)
        }
    }
}

/// Wrapper that adds a stable UUID so SwiftUI `ForEach` is happy when
/// rendering streams of events.
struct IdentifiedWsEvent: Identifiable, Hashable {
    let id = UUID()
    let event: WsStreamEvent
}

struct IdentifiedSseEvent: Identifiable, Hashable {
    let id = UUID()
    let event: SseStreamEvent
}
