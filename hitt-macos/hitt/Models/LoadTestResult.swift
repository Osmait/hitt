import Foundation

/// Mirrors `hitt_core::testing::load_test::LoadTestResult`.
/// All `Duration` fields are serialized as milliseconds (u64) by the Rust side.
struct LoadTestResult: Codable, Hashable {
    var totalRequests: UInt32
    var successful: UInt32
    var failed: UInt32
    var errorRate: Double
    /// Total wall-clock duration in ms.
    var duration: UInt64
    var rps: Double
    var latency: LatencyStats
    var statusDistribution: [String: UInt32]
    var errors: [String]

    enum CodingKeys: String, CodingKey {
        case totalRequests = "total_requests"
        case successful, failed
        case errorRate = "error_rate"
        case duration, rps, latency
        case statusDistribution = "status_distribution"
        case errors
    }
}

struct LatencyStats: Codable, Hashable {
    var min: UInt64
    var max: UInt64
    var mean: UInt64
    var median: UInt64
    var p90: UInt64
    var p95: UInt64
    var p99: UInt64
    var stdDev: UInt64

    enum CodingKeys: String, CodingKey {
        case min, max, mean, median, p90, p95, p99
        case stdDev = "std_dev"
    }
}
