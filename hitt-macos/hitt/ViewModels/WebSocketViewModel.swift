import Foundation

@Observable
final class WebSocketViewModel {
    var url: String = "wss://echo.websocket.events"
    var outgoingMessage: String = ""
    var events: [IdentifiedWsEvent] = []
    var sessionId: String?
    var isConnected: Bool { sessionId != nil }
    var errorMessage: String?

    private var service: HittService { HittService.shared }
    private var pollTask: Task<Void, Never>?

    func connect() async {
        errorMessage = nil
        events.removeAll()
        do {
            let id = try await service.wsConnect(url: url)
            sessionId = id
            startPolling(sessionId: id)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func send() async {
        guard let id = sessionId, !outgoingMessage.isEmpty else { return }
        let msg = outgoingMessage
        outgoingMessage = ""
        do {
            try await service.wsSend(sessionId: id, text: msg)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func disconnect() async {
        pollTask?.cancel()
        pollTask = nil
        if let id = sessionId {
            try? await service.wsDisconnect(sessionId: id)
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
                    let newEvents = try await self.service.wsPoll(sessionId: sessionId)
                    for ev in newEvents {
                        self.events.append(IdentifiedWsEvent(event: ev))
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
