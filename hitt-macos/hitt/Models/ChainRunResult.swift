import Foundation

/// Mirrors `hitt_core::ChainRunResult`.
struct ChainRunResult: Codable, Hashable {
    var steps: [ChainStepResult]
    var finalVars: [String: String]

    enum CodingKeys: String, CodingKey {
        case steps
        case finalVars = "final_vars"
    }
}

struct ChainStepResult: Codable, Hashable, Identifiable {
    var stepIndex: UInt32
    var status: ChainStepStatus
    var requestName: String?
    var httpStatus: UInt16?
    var durationMs: UInt64?
    var extracted: [String: String]
    var error: String?

    var id: UInt32 { stepIndex }

    enum CodingKeys: String, CodingKey {
        case stepIndex = "step_index"
        case status
        case requestName = "request_name"
        case httpStatus = "http_status"
        case durationMs = "duration_ms"
        case extracted, error
    }
}

enum ChainStepStatus: String, Codable, Hashable {
    case success = "Success"
    case failed = "Failed"
    case skipped = "Skipped"
}
