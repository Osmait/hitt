import SwiftUI

struct SidebarView: View {
    @Environment(AppViewModel.self) private var appVM
    @State private var isShowingNewCollection: Bool = false
    @State private var newCollectionName: String = ""

    var body: some View {
        VStack(spacing: 0) {
            // Environment picker at the top
            HStack {
                EnvironmentPicker()
                Spacer()
            }
            .padding(.horizontal, HittTheme.Spacing.sm)
            .padding(.top, HittTheme.Spacing.sm)
            .padding(.bottom, HittTheme.Spacing.xs)

            // Collections section header with Import menu + Add
            HStack {
                Text("COLLECTIONS")
                    .font(HittTheme.Typography.tiny)
                    .tracking(0.5)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
                Spacer()
                ImportMenu()
                Button {
                    isShowingNewCollection = true
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(HittTheme.Colors.textSecondary)
                }
                .buttonStyle(.hoverIcon)
            }
            .padding(.horizontal, HittTheme.Spacing.sm)
            .padding(.vertical, HittTheme.Spacing.xs)

            // Search
            if appVM.collections.collections.count >= 3 {
                SearchFieldView(
                    text: Binding(
                        get: { appVM.collections.filter },
                        set: { appVM.collections.filter = $0 }
                    ),
                    placeholder: "Search…"
                )
                .padding(.horizontal, HittTheme.Spacing.sm)
                .padding(.bottom, HittTheme.Spacing.xs)
            }

            // Collections list
            if appVM.collections.collections.isEmpty {
                EmptyStateView(
                    icon: "tray",
                    title: "No collections",
                    subtitle: "Click + to create your first one"
                )
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 2) {
                        ForEach(appVM.collections.filteredCollections) { col in
                            CollectionRow(collection: col)
                        }
                    }
                    .padding(.horizontal, HittTheme.Spacing.xs)
                }
            }
        }
        .sheet(isPresented: $isShowingNewCollection) {
            newCollectionSheet
        }
    }

    private var newCollectionSheet: some View {
        VStack(alignment: .leading, spacing: HittTheme.Spacing.md) {
            Text("New collection")
                .font(HittTheme.Typography.headline)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            TextField("Name", text: $newCollectionName)
                .textFieldStyle(.roundedBorder)
                .onSubmit(create)
            HStack {
                Spacer()
                Button("Cancel") {
                    isShowingNewCollection = false
                    newCollectionName = ""
                }
                .buttonStyle(.plain)
                .foregroundStyle(HittTheme.Colors.textSecondary)
                Button("Create", action: create)
                    .buttonStyle(.primary)
                    .disabled(newCollectionName.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(HittTheme.Spacing.lg)
        .frame(width: 340)
    }

    private func create() {
        let name = newCollectionName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        Task { await appVM.createCollection(name: name) }
        isShowingNewCollection = false
        newCollectionName = ""
    }
}
