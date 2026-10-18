import Foundation

/// Environment list + active selection.
@Observable
final class EnvironmentsViewModel {
    var environments: [HittEnvironment] = []
    var activeEnvironmentName: String? {
        didSet {
            UserDefaults.standard.set(activeEnvironmentName, forKey: Self.storageKey)
        }
    }

    private static let storageKey = "activeEnvironment"
    private var service: HittService { HittService.shared }

    init() {
        self.activeEnvironmentName = UserDefaults.standard.string(forKey: Self.storageKey)
    }

    var activeEnvironment: HittEnvironment? {
        guard let name = activeEnvironmentName else { return nil }
        return environments.first { $0.name == name }
    }

    func load() async throws {
        environments = try await service.loadEnvironments()
        // If the previously active env no longer exists, clear it.
        if let name = activeEnvironmentName,
           !environments.contains(where: { $0.name == name })
        {
            activeEnvironmentName = nil
        }
        try await service.setActiveEnvironment(name: activeEnvironmentName)
    }

    func setActive(_ name: String?) async throws {
        activeEnvironmentName = name
        try await service.setActiveEnvironment(name: name)
    }

    func create(name: String) async throws {
        let env = HittEnvironment.new(name: name)
        let saved = try await service.saveEnvironment(env)
        if let idx = environments.firstIndex(where: { $0.name == saved.name }) {
            environments[idx] = saved
        } else {
            environments.append(saved)
        }
    }

    @discardableResult
    func save(_ env: HittEnvironment) async throws -> HittEnvironment {
        let saved = try await service.saveEnvironment(env)
        if let idx = environments.firstIndex(where: { $0.id == saved.id }) {
            environments[idx] = saved
        } else {
            environments.append(saved)
        }
        return saved
    }
}
