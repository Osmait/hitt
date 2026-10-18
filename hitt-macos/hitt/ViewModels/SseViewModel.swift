import Foundation

@Observable
final class SseViewModel {
    var url: String = "https://sse.dev/test"
    var events: [IdentifiedSseEvent] = []
    var sessionId: String?
    var isConnected: Bool { sessionId != nil }
    var errorMessage: String?

    /// Optional: concatenate all `Event.data` payloads (useful for AI streaming APIs).
    var accumulateMode: Bool = false
    var accumulatedText: String = ""

    private var service: HittService { HittService.shared }
    private var pollTask: Task<Void, Never>?

    func connect() async {
        errorMessage = nil
        events.removeAll()
        accumulatedText = ""
        do {
            let id = try await service.sseConnect(url: url)
            sessionId = id
            startPolling(sessionId: id)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func disconnect() async {
        pollTask?.cancel()
        pollTask = nil
        if let id = sessionId {
            try? await service.sseDisconnect(sessionId: id)
        }
        sessionId = nil
    }

    private func startPolling(sessionId: String) {
        pollTask?.cancel()
        pollTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                if self.sessionId != sessionId { return }
                do {
                    let newEvents = try await self.service.ssePoll(sessionId: sessionId)
                    for ev in newEvents {
                        self.events.append(IdentifiedSseEvent(event: ev))
                        if case .event(_, _, let data, _) = ev, self.accumulateMode {
                            self.accumulatedText += data
                        }
                        if case .closed = ev {
                            self.sessionId = nil
                            return
                        }
                    }
                } catch {
                    self.errorMessage = error.localizedDescription
                }
                try? await Task.sleep(for: .milliseconds(150))
            }
        }
    }
}
