import SwiftUI

/// Assertions editor — add/remove/edit request-level assertions.
struct AssertionsTab: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        @Bindable var editor = appVM.editor
        VStack(spacing: 0) {
            if editor.request.assertions.isEmpty {
                EmptyStateView(
                    icon: "checkmark.seal",
                    title: "No tests",
                    subtitle: "Add assertions to validate every response automatically."
                )
            } else {
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach($editor.request.assertions) { $a in
                            assertionRow($a)
                        }
                    }
                    .padding(HittTheme.Spacing.sm)
                }
            }
            addRow
        }
    }

    private func assertionRow(_ assertion: Binding<AssertionModel>) -> some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            Toggle(isOn: assertion.enabled) { EmptyView() }
                .toggleStyle(.checkbox)
                .labelsHidden()

            editor(for: assertion)
                .frame(maxWidth: .infinity, alignment: .leading)

            Button {
                appVM.editor.request.assertions.removeAll { $0.id == assertion.wrappedValue.id }
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .buttonStyle(.hoverIcon)
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 4)
        .background(HittTheme.Colors.surfaceElevated.opacity(0.3))
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }

    @ViewBuilder
    private func editor(for assertion: Binding<AssertionModel>) -> some View {
        switch assertion.wrappedValue.kind {
        case .statusEquals(let status):
            HStack(spacing: 6) {
                Text("status ==").font(HittTheme.Typography.mono).foregroundStyle(HittTheme.Colors.textSecondary)
                TextField("200", value: Binding<Int>(
                    get: { Int(status) },
                    set: { new in
                        var copy = assertion.wrappedValue
                        copy.kind = .statusEquals(UInt16(max(0, min(65535, new))))
                        assertion.wrappedValue = copy
                    }
                ), formatter: NumberFormatter())
                .textFieldStyle(.roundedBorder)
                .frame(width: 80)
                .font(HittTheme.Typography.mono)
            }
        case .bodyContains(let s):
            HStack(spacing: 6) {
                Text("body contains").font(HittTheme.Typography.mono).foregroundStyle(HittTheme.Colors.textSecondary)
                TextField("substring", text: Binding(
                    get: { s },
                    set: {
                        var copy = assertion.wrappedValue
                        copy.kind = .bodyContains($0)
                        assertion.wrappedValue = copy
                    }
                ))
                .textFieldStyle(.roundedBorder)
                .font(HittTheme.Typography.mono)
            }
        case .headerExists(let h):
            HStack(spacing: 6) {
                Text("header").font(HittTheme.Typography.mono).foregroundStyle(HittTheme.Colors.textSecondary)
                TextField("Name", text: Binding(
                    get: { h },
                    set: {
                        var copy = assertion.wrappedValue
                        copy.kind = .headerExists($0)
                        assertion.wrappedValue = copy
                    }
                ))
                .textFieldStyle(.roundedBorder)
                .font(HittTheme.Typography.mono)
                Text("exists").font(HittTheme.Typography.mono).foregroundStyle(HittTheme.Colors.textSecondary)
            }
        case .responseTimeLessThan(let ms):
            HStack(spacing: 6) {
                Text("time <").font(HittTheme.Typography.mono).foregroundStyle(HittTheme.Colors.textSecondary)
                TextField("ms", value: Binding<Int>(
                    get: { Int(ms) },
                    set: { new in
                        var copy = assertion.wrappedValue
                        copy.kind = .responseTimeLessThan(UInt64(max(0, new)))
                        assertion.wrappedValue = copy
                    }
                ), formatter: NumberFormatter())
                .textFieldStyle(.roundedBorder)
                .frame(width: 90)
                .font(HittTheme.Typography.mono)
                Text("ms").font(HittTheme.Typography.mono).foregroundStyle(HittTheme.Colors.textSecondary)
            }
        default:
            Text(assertion.wrappedValue.kind.displayLabel)
                .font(HittTheme.Typography.mono)
                .foregroundStyle(HittTheme.Colors.textSecondary)
        }
    }

    private var addRow: some View {
        HStack {
            Menu {
                Button("Status equals…") { add(.statusEquals(200)) }
                Button("Body contains…") { add(.bodyContains("")) }
                Button("Header exists…") { add(.headerExists("")) }
                Button("Response time <…") { add(.responseTimeLessThan(1000)) }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "plus.circle").font(.system(size: 11))
                    Text("Add assertion").font(HittTheme.Typography.bodyMedium)
                }
                .foregroundStyle(HittTheme.Colors.textSecondary)
                .padding(.horizontal, HittTheme.Spacing.sm)
                .padding(.vertical, HittTheme.Spacing.xs)
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            Spacer()
        }
        .padding(HittTheme.Spacing.sm)
    }

    private func add(_ kind: AssertionKindModel) {
        let a = AssertionModel(
            id: UUID().uuidString.lowercased(),
            kind: kind,
            enabled: true
        )
        appVM.editor.request.assertions.append(a)
    }
}
