import SwiftUI

/// Full-featured env editor: add/remove/enable/toggle-secret/edit variables.
struct EnvironmentEditorSheet: View {
    @Environment(AppViewModel.self) private var appVM
    @Environment(\.dismiss) private var dismiss

    @State private var workingEnv: HittEnvironment?

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().background(HittTheme.Colors.borderSubtle)
            if let env = workingEnv {
                nameBar(env: env)
                Divider().background(HittTheme.Colors.borderSubtle)
                varsTable
                Divider().background(HittTheme.Colors.borderSubtle)
                actionBar
            } else {
                EmptyStateView(icon: "leaf", title: "Pick an environment to edit").frame(height: 200)
            }
        }
        .frame(minWidth: 640, minHeight: 460)
        .background(HittTheme.Colors.surface)
        .onAppear(perform: load)
    }

    private var header: some View {
        HStack {
            Text("Edit Environment")
                .font(HittTheme.Typography.title)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            Spacer()
            Button { dismiss() } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 16))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .buttonStyle(.plain)
        }
        .padding(HittTheme.Spacing.lg)
    }

    private func nameBar(env: HittEnvironment) -> some View {
        HStack {
            Text("Name")
                .font(HittTheme.Typography.caption)
                .foregroundStyle(HittTheme.Colors.textTertiary)
                .frame(width: 60, alignment: .leading)
            TextField("Env name", text: Binding(
                get: { workingEnv?.name ?? "" },
                set: { workingEnv?.name = $0 }
            ))
            .textFieldStyle(.roundedBorder)
            .font(HittTheme.Typography.mono)
        }
        .padding(HittTheme.Spacing.md)
    }

    private var varsTable: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                Color.clear.frame(width: 32)
                Text("KEY")
                    .font(HittTheme.Typography.tiny).tracking(0.5)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text("VALUE")
                    .font(HittTheme.Typography.tiny).tracking(0.5)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text("SECRET")
                    .font(HittTheme.Typography.tiny).tracking(0.5)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
                    .frame(width: 56)
                Color.clear.frame(width: 28)
            }
            .padding(.horizontal, HittTheme.Spacing.sm)
            .padding(.vertical, 6)
            .background(HittTheme.Colors.surface.opacity(0.5))
            Divider().background(HittTheme.Colors.borderSubtle)
            ScrollView {
                LazyVStack(spacing: 0) {
                    if let env = workingEnv {
                        ForEach(Array(env.values.enumerated()), id: \.element.id) { idx, _ in
                            row(index: idx)
                            Divider().background(HittTheme.Colors.borderSubtle.opacity(0.3))
                        }
                    }
                }
            }
        }
    }

    private func row(index idx: Int) -> some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            Toggle(isOn: Binding(
                get: { workingEnv?.values[idx].enabled ?? true },
                set: { workingEnv?.values[idx].enabled = $0 }
            )) { EmptyView() }
                .toggleStyle(.checkbox)
                .labelsHidden()

            TextField("KEY", text: Binding(
                get: { workingEnv?.values[idx].key ?? "" },
                set: { workingEnv?.values[idx].key = $0 }
            ))
            .textFieldStyle(.plain)
            .font(HittTheme.Typography.mono)
            .frame(maxWidth: .infinity, alignment: .leading)

            if workingEnv?.values[idx].secret == true {
                SecureField("value", text: Binding(
                    get: { workingEnv?.values[idx].value ?? "" },
                    set: { workingEnv?.values[idx].value = $0 }
                ))
                .textFieldStyle(.plain)
                .font(HittTheme.Typography.mono)
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                TextField("value", text: Binding(
                    get: { workingEnv?.values[idx].value ?? "" },
                    set: { workingEnv?.values[idx].value = $0 }
                ))
                .textFieldStyle(.plain)
                .font(HittTheme.Typography.mono)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Toggle(isOn: Binding(
                get: { workingEnv?.values[idx].secret ?? false },
                set: { workingEnv?.values[idx].secret = $0 }
            )) { EmptyView() }
                .toggleStyle(.switch)
                .labelsHidden()
                .controlSize(.small)
                .frame(width: 56)

            Button {
                workingEnv?.values.remove(at: idx)
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .buttonStyle(.hoverIcon)
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 4)
    }

    private var actionBar: some View {
        HStack {
            Button {
                workingEnv?.values.append(EnvironmentVariable())
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "plus.circle").font(.system(size: 11))
                    Text("Add variable").font(HittTheme.Typography.bodyMedium)
                }
                .foregroundStyle(HittTheme.Colors.textSecondary)
                .padding(.horizontal, HittTheme.Spacing.sm)
                .padding(.vertical, HittTheme.Spacing.xs)
            }
            .buttonStyle(.hover)
            Spacer()
            Button("Cancel") { dismiss() }
                .buttonStyle(.plain)
                .foregroundStyle(HittTheme.Colors.textSecondary)
            Button("Save") { save() }
                .buttonStyle(.primary)
                .disabled((workingEnv?.name.trimmingCharacters(in: .whitespaces).isEmpty) ?? true)
        }
        .padding(HittTheme.Spacing.md)
    }

    private func load() {
        guard let id = appVM.editingEnvironmentId else { return }
        workingEnv = appVM.environments.environments.first { $0.id == id }
    }

    private func save() {
        guard let env = workingEnv else { return }
        Task {
            do {
                try await appVM.environments.save(env)
                appVM.showToast("Environment saved")
                dismiss()
            } catch {
                appVM.showError(error)
            }
        }
    }
}
