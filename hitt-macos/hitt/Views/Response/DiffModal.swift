import SwiftUI

struct DiffModal: View {
    @Environment(AppViewModel.self) private var appVM
    @Environment(\.dismiss) private var dismiss

    @State private var leftIndex: Int? = nil
    @State private var rightIndex: Int? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().background(HittTheme.Colors.borderSubtle)
            pickers
            Divider().background(HittTheme.Colors.borderSubtle)
            diffContent
        }
        .frame(minWidth: 720, minHeight: 480)
        .background(HittTheme.Colors.surface)
        .onAppear(perform: pickDefaults)
    }

    private var header: some View {
        HStack {
            Text("Diff").font(HittTheme.Typography.title)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            Spacer()
            if let d = appVM.lastDiffResult {
                statsBadge(d)
            }
            Button { dismiss() } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 16))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .buttonStyle(.plain)
        }
        .padding(HittTheme.Spacing.lg)
    }

    private func statsBadge(_ d: DiffResult) -> some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            Label("+\(d.additions)", systemImage: "plus.circle")
                .labelStyle(.titleAndIcon)
                .font(HittTheme.Typography.captionBold)
                .foregroundStyle(HittTheme.Colors.success)
            Label("−\(d.deletions)", systemImage: "minus.circle")
                .labelStyle(.titleAndIcon)
                .font(HittTheme.Typography.captionBold)
                .foregroundStyle(HittTheme.Colors.danger)
        }
    }

    private var pickers: some View {
        HStack(spacing: HittTheme.Spacing.md) {
            picker(label: "Left", selection: $leftIndex)
            Image(systemName: "arrow.left.arrow.right")
                .font(.system(size: 11))
                .foregroundStyle(HittTheme.Colors.textTertiary)
            picker(label: "Right", selection: $rightIndex)
            Spacer()
            Button("Compute") { compute() }
                .buttonStyle(.primary)
                .disabled(leftIndex == nil || rightIndex == nil)
        }
        .padding(HittTheme.Spacing.md)
    }

    private func picker(label: String, selection: Binding<Int?>) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(HittTheme.Typography.tiny)
                .tracking(0.5)
                .foregroundStyle(HittTheme.Colors.textTertiary)
            Menu {
                ForEach(Array(appVM.responseHistory.enumerated()), id: \.offset) { idx, entry in
                    Button("#\(idx + 1) — \(entry.name)") { selection.wrappedValue = idx }
                }
            } label: {
                HStack {
                    Text(selectionLabel(selection.wrappedValue))
                        .font(HittTheme.Typography.mono)
                        .lineLimit(1)
                    Spacer()
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(HittTheme.Colors.textTertiary)
                }
                .padding(.horizontal, HittTheme.Spacing.sm)
                .padding(.vertical, 6)
                .frame(width: 250)
                .background(HittTheme.Colors.background.opacity(0.5))
                .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
        }
    }

    @ViewBuilder private var diffContent: some View {
        if let d = appVM.lastDiffResult {
            ScrollView([.vertical, .horizontal]) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(d.lines) { line in
                        diffLineRow(line)
                    }
                }
                .padding(HittTheme.Spacing.xs)
            }
            .background(HittTheme.Colors.background.opacity(0.4))
        } else {
            EmptyStateView(
                icon: "doc.on.doc",
                title: "Pick two responses",
                subtitle: "Both sides come from the recent request history (last 20)."
            )
        }
    }

    private func diffLineRow(_ line: DiffLine) -> some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            Text(line.oldLine.map(String.init) ?? "")
                .font(HittTheme.Typography.monoSmall)
                .foregroundStyle(HittTheme.Colors.textTertiary)
                .frame(width: 32, alignment: .trailing)
            Text(line.newLine.map(String.init) ?? "")
                .font(HittTheme.Typography.monoSmall)
                .foregroundStyle(HittTheme.Colors.textTertiary)
                .frame(width: 32, alignment: .trailing)
            Text(prefix(for: line.tag))
                .font(HittTheme.Typography.monoBold)
                .foregroundStyle(color(for: line.tag))
                .frame(width: 12)
            Text(line.content)
                .font(HittTheme.Typography.mono)
                .foregroundStyle(HittTheme.Colors.textPrimary)
                .textSelection(.enabled)
            Spacer()
        }
        .padding(.horizontal, HittTheme.Spacing.sm)
        .padding(.vertical, 2)
        .background(background(for: line.tag))
    }

    private func prefix(for tag: DiffTag) -> String {
        switch tag {
        case .equal: " "
        case .insert: "+"
        case .delete: "−"
        }
    }

    private func color(for tag: DiffTag) -> Color {
        switch tag {
        case .equal: HittTheme.Colors.textTertiary
        case .insert: HittTheme.Colors.success
        case .delete: HittTheme.Colors.danger
        }
    }

    private func background(for tag: DiffTag) -> Color {
        switch tag {
        case .equal: Color.clear
        case .insert: HittTheme.Colors.success.opacity(0.08)
        case .delete: HittTheme.Colors.danger.opacity(0.08)
        }
    }

    private func selectionLabel(_ index: Int?) -> String {
        guard let i = index, i < appVM.responseHistory.count else { return "—" }
        return "#\(i + 1) \(appVM.responseHistory[i].name)"
    }

    private func pickDefaults() {
        let count = appVM.responseHistory.count
        if count >= 2 {
            leftIndex = count - 2
            rightIndex = count - 1
        } else if count == 1 {
            rightIndex = 0
        }
    }

    private func compute() {
        guard let li = leftIndex, let ri = rightIndex,
              li < appVM.responseHistory.count,
              ri < appVM.responseHistory.count else { return }
        let left = appVM.responseHistory[li].body
        let right = appVM.responseHistory[ri].body
        Task { await appVM.runDiff(left: left, right: right) }
    }
}
