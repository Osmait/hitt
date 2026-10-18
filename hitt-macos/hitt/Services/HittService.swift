import Foundation

/// Thread-safe actor wrapping the Rust `HittEngine` (UniFFI).
/// Swift domain types are encoded/decoded via JSON on the FFI boundary.
actor HittService {
    static let shared = HittService()

    private let engine: HittEngine
    private let encoder: JSONEncoder
    private let decoder: JSONDecoder

    private init() {
        self.engine = HittEngine()
        self.encoder = JSONEncoder()
        self.encoder.outputFormatting = []
        self.decoder = JSONDecoder()
    }

    // MARK: - Collections

    func loadCollections() async throws -> [HittCollection] {
        let json = try await engine.loadCollectionsJson()
        guard let data = json.data(using: .utf8) else { return [] }
        return try decoder.decode([HittCollection].self, from: data)
    }

    @discardableResult
    func saveCollection(_ collection: HittCollection) async throws -> HittCollection {
        let data = try encoder.encode(collection)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        let resultJson = try await engine.saveCollectionJson(collectionJson: json)
        guard let resultData = resultJson.data(using: .utf8) else {
            throw HittError.encoding
        }
        return try decoder.decode(HittCollection.self, from: resultData)
    }

    func deleteCollection(name: String) async throws {
        try await engine.deleteCollection(name: name)
    }

    // MARK: - Environments

    func loadEnvironments() async throws -> [HittEnvironment] {
        let json = try await engine.loadEnvironmentsJson()
        guard let data = json.data(using: .utf8) else { return [] }
        return try decoder.decode([HittEnvironment].self, from: data)
    }

    @discardableResult
    func saveEnvironment(_ env: HittEnvironment) async throws -> HittEnvironment {
        let data = try encoder.encode(env)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        let resultJson = try await engine.saveEnvironmentJson(environmentJson: json)
        guard let resultData = resultJson.data(using: .utf8) else {
            throw HittError.encoding
        }
        return try decoder.decode(HittEnvironment.self, from: resultData)
    }

    func setActiveEnvironment(name: String?) async throws {
        try await engine.setActiveEnvironment(name: name)
    }

    // MARK: - HTTP

    /// Returns the fast projection plus the full parsed `Response`.
    func sendRequest(_ request: Request) async throws -> (FfiResponse, Response) {
        let data = try encoder.encode(request)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        let ffi = try await engine.sendRequest(requestJson: json)
        guard let fullData = ffi.fullJson.data(using: .utf8) else {
            throw HittError.encoding
        }
        let full = try decoder.decode(Response.self, from: fullData)
        return (ffi, full)
    }

    // MARK: - Testing

    func runLoadTest(
        _ request: Request,
        total: UInt32,
        concurrency: UInt32,
        timeoutMs: UInt64
    ) async throws -> LoadTestResult {
        let data = try encoder.encode(request)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        let resultJson = try await engine.runLoadTest(
            requestJson: json,
            total: total,
            concurrency: concurrency,
            timeoutMs: timeoutMs
        )
        guard let resultData = resultJson.data(using: .utf8) else {
            throw HittError.encoding
        }
        return try decoder.decode(LoadTestResult.self, from: resultData)
    }

    func runDiff(left: String, right: String) async throws -> DiffResult {
        let resultJson = try await engine.runDiff(left: left, right: right)
        guard let data = resultJson.data(using: .utf8) else {
            throw HittError.encoding
        }
        return try decoder.decode(DiffResult.self, from: data)
    }

    func runChain(collectionId: String, chainName: String) async throws -> ChainRunResult {
        let resultJson = try await engine.runChain(collectionId: collectionId, chainName: chainName)
        guard let data = resultJson.data(using: .utf8) else {
            throw HittError.encoding
        }
        return try decoder.decode(ChainRunResult.self, from: data)
    }

    // MARK: - WebSocket

    func wsConnect(url: String, headers: [KeyValuePair] = []) async throws -> String {
        let headersJson = try encodeHeaders(headers)
        return try await engine.wsConnect(url: url, headersJson: headersJson)
    }

    func wsSend(sessionId: String, text: String) async throws {
        try await engine.wsSend(sessionId: sessionId, text: text)
    }

    func wsPoll(sessionId: String) async throws -> [WsStreamEvent] {
        let json = try await engine.wsPoll(sessionId: sessionId)
        guard let data = json.data(using: .utf8) else { return [] }
        return try decoder.decode([WsStreamEvent].self, from: data)
    }

    func wsDisconnect(sessionId: String) async throws {
        try await engine.wsDisconnect(sessionId: sessionId)
    }

    // MARK: - SSE

    func sseConnect(url: String, headers: [KeyValuePair] = []) async throws -> String {
        let headersJson = try encodeHeaders(headers)
        return try await engine.sseConnect(url: url, headersJson: headersJson)
    }

    func ssePoll(sessionId: String) async throws -> [SseStreamEvent] {
        let json = try await engine.ssePoll(sessionId: sessionId)
        guard let data = json.data(using: .utf8) else { return [] }
        return try decoder.decode([SseStreamEvent].self, from: data)
    }

    func sseDisconnect(sessionId: String) async throws {
        try await engine.sseDisconnect(sessionId: sessionId)
    }

    private func encodeHeaders(_ headers: [KeyValuePair]) throws -> String {
        let data = try encoder.encode(headers)
        return String(data: data, encoding: .utf8) ?? "[]"
    }

    // MARK: - History

    func getHistory() async throws -> [HistoryEntry] {
        let json = try await engine.getHistoryJson()
        guard let data = json.data(using: .utf8) else { return [] }
        return try decoder.decode([HistoryEntry].self, from: data)
    }

    func clearHistory() async throws {
        try await engine.clearHistory()
    }

    // MARK: - Importers (synchronous — parsers don't touch network)

    nonisolated func importPostmanCollection(_ content: String) throws -> HittCollection {
        let json = try engine.importPostmanCollection(content: content)
        return try decodeJson(json, as: HittCollection.self)
    }

    nonisolated func importPostmanEnvironment(_ content: String) throws -> HittEnvironment {
        let json = try engine.importPostmanEnvironment(content: content)
        return try decodeJson(json, as: HittEnvironment.self)
    }

    nonisolated func importOpenAPI(_ content: String) throws -> HittCollection {
        let json = try engine.importOpenapi(content: content)
        return try decodeJson(json, as: HittCollection.self)
    }

    nonisolated func importHar(_ content: String) throws -> HittCollection {
        let json = try engine.importHar(content: content)
        return try decodeJson(json, as: HittCollection.self)
    }

    nonisolated func importCurl(_ content: String) throws -> Request {
        let json = try engine.importCurl(content: content)
        return try decodeJson(json, as: Request.self)
    }

    nonisolated func importDotenv(_ content: String, envName: String) throws -> HittEnvironment {
        let json = try engine.importDotenv(content: content, envName: envName)
        return try decodeJson(json, as: HittEnvironment.self)
    }

    nonisolated private func decodeJson<T: Decodable>(_ json: String, as type: T.Type) throws -> T {
        guard let data = json.data(using: .utf8) else { throw HittError.encoding }
        return try JSONDecoder().decode(T.self, from: data)
    }

    // MARK: - Exporters

    nonisolated func exportToCurl(_ request: Request) throws -> String {
        let data = try JSONEncoder().encode(request)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        return try engine.exportToCurl(requestJson: json)
    }

    nonisolated func exportToMarkdown(_ collection: HittCollection) throws -> String {
        let data = try JSONEncoder().encode(collection)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        return try engine.exportToMarkdown(collectionJson: json)
    }

    nonisolated func exportPostmanCollection(_ collection: HittCollection) throws -> String {
        let data = try JSONEncoder().encode(collection)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        return try engine.exportPostmanCollection(collectionJson: json)
    }

    nonisolated func exportPostmanEnvironment(_ env: HittEnvironment) throws -> String {
        let data = try JSONEncoder().encode(env)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        return try engine.exportPostmanEnvironment(envJson: json)
    }
}

enum HittError: LocalizedError {
    case encoding

    var errorDescription: String? {
        switch self {
        case .encoding: "Failed to encode/decode JSON between Swift and Rust"
        }
    }
}
