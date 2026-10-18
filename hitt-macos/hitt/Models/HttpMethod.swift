import SwiftUI

/// Mirrors `hitt_core::core::request::HttpMethod`.
enum HttpMethod: String, Codable, CaseIterable, Identifiable {
    case GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, TRACE

    var id: String { rawValue }

    var color: Color {
        switch self {
        case .GET: HittTheme.Colors.methodGet
        case .POST: HittTheme.Colors.methodPost
        case .PUT: HittTheme.Colors.methodPut
        case .PATCH: HittTheme.Colors.methodPatch
        case .DELETE: HittTheme.Colors.methodDelete
        case .HEAD: HittTheme.Colors.methodHead
        case .OPTIONS: HittTheme.Colors.methodOptions
        case .TRACE: HittTheme.Colors.textSecondary
        }
    }
}
