import SwiftUI

/// Menu to switch the active environment.
struct EnvironmentPicker: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        Menu {
            Button("No environment") {
                Task { await appVM.setEnvironment(nil) }
            }
            Divider()
            Section("Switch") {
                ForEach(appVM.environments.environments) { env in
                    Button(env.name) {
                        Task { await appVM.setEnvironment(env.name) }
                    }
                }
            }
            Divider()
            Section("Edit") {
                ForEach(appVM.environments.environments) { env in
                    Button("Edit '\(env.name)'…") {
                        appVM.openEnvironmentEditor(env.id)
                    }
                }
                Button("New environment…") {
                    Task {
                        do {
                            try await appVM.environments.create(name: "untitled")
                            if let last = appVM.environments.environments.last {
                                appVM.openEnvironmentEditor(last.id)
                            }
                        } catch {
                            appVM.showError(error)
                        }
                    }
                }
            }
            if !appVM.environments.environments.isEmpty {
                Divider()
                Section("Export") {
                    ForEach(appVM.environments.environments) { env in
                        Button("Export '\(env.name)' as Postman…") {
                            appVM.exportEnvironmentAsPostman(env)
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "leaf.fill")
                    .font(.system(size: 10))
                    .foregroundStyle(HittTheme.Colors.success.opacity(0.8))
                Text(appVM.environments.activeEnvironmentName ?? "No env")
                    .font(HittTheme.Typography.captionBold)
                    .foregroundStyle(HittTheme.Colors.textPrimary)
                    .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .padding(.horizontal, HittTheme.Spacing.sm)
            .padding(.vertical, 4)
            .background(HittTheme.Colors.surfaceElevated)
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }
}
