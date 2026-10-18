import Foundation

/// Sidebar state: the list of collections and user selection.
@Observable
final class CollectionsViewModel {
    var collections: [HittCollection] = []
    var selectedRequestId: String?
    var filter: String = ""
    var isShowingNewCollectionSheet: Bool = false
    var newCollectionName: String = ""

    private var service: HittService { HittService.shared }

    /// Flattened across collections — useful for searching.
    var allRequests: [(collection: HittCollection, request: Request)] {
        collections.flatMap { col in
            col.allRequests.map { (collection: col, request: $0) }
        }
    }

    var filteredCollections: [HittCollection] {
        guard !filter.isEmpty else { return collections }
        return collections.compactMap { col -> HittCollection? in
            if col.name.localizedCaseInsensitiveContains(filter) { return col }
            let matches = col.allRequests.filter {
                $0.name.localizedCaseInsensitiveContains(filter) ||
                $0.url.localizedCaseInsensitiveContains(filter)
            }
            if matches.isEmpty { return nil }
            var copy = col
            copy.items = matches.map { .request($0) }
            return copy
        }
    }

    func load() async throws {
        collections = try await service.loadCollections()
    }

    func createCollection(name: String) async throws {
        let new = HittCollection.new(name: name)
        let saved = try await service.saveCollection(new)
        if let idx = collections.firstIndex(where: { $0.name == saved.name }) {
            collections[idx] = saved
        } else {
            collections.append(saved)
        }
    }

    func delete(_ collection: HittCollection) async throws {
        try await service.deleteCollection(name: collection.name)
        collections.removeAll { $0.id == collection.id }
    }

    /// Upsert a collection that came from an import (de-duplicates by name).
    @discardableResult
    func upsertImported(_ imported: HittCollection) async throws -> HittCollection {
        let saved = try await service.saveCollection(imported)
        if let idx = collections.firstIndex(where: { $0.id == saved.id }) {
            collections[idx] = saved
        } else if let idx = collections.firstIndex(where: { $0.name == saved.name }) {
            collections[idx] = saved
        } else {
            collections.append(saved)
        }
        return saved
    }

    /// Persist a request back into its parent collection (Phase 2).
    func saveRequest(_ request: Request, into collectionId: String) async throws {
        guard let idx = collections.firstIndex(where: { $0.id == collectionId }) else { return }
        var col = collections[idx]
        // Upsert by id, flat (ignore folder nesting in Phase 1/2)
        var replaced = false
        col.items = col.items.map { item -> CollectionItem in
            if case .request(let r) = item, r.id == request.id {
                replaced = true
                return .request(request)
            }
            return item
        }
        if !replaced {
            col.items.append(.request(request))
        }
        let saved = try await service.saveCollection(col)
        collections[idx] = saved
    }
}
