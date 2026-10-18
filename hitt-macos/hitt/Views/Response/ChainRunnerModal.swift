import SwiftUI

struct ChainRunnerModal: View {
    @Environment(AppViewModel.self) private var appVM
    @Environment(\.dismiss) private var dismiss

    @State private var selectedChain: ChainSelection?

    struct ChainSelection: Hashable {
        let collectionId: String
        let collectionName: String
        let chainName: String
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().background(HittTheme.Colors.borderSubtle)
            picker
            Divider().background(HittTheme.Colors.borderSubtle)
            resultsContent
        }
        .frame(minWidth: 640, minHeight: 480)
        .background(HittTheme.Colors.surface)
    }

    private var header: some View {
        HStack {
            Text("Run Chain")
                .font(HittTheme.Typography.title)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            Spacer()
            if let r = appVM.lastChainResult {
                let ok = r.steps.filter { $0.status == .success }.count
                let fail = r.steps.filter { $0.status == .failed }.count
                let skip = r.steps.filter { $0.status == .skipped }.count
                HStack(spacing: 6) {
                    badge(text: "\(ok) ok", color: HittTheme.Colors.success)
                    if fail > 0 {
                        badge(text: "\(fail) fail", color: HittTheme.Colors.danger)
                    }
                    if skip > 0 {
                        badge(text: "\(skip) skip", color: HittTheme.Colors.warning)
                    }
                }
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

    private var picker: some View {
        HStack(spacing: HittTheme.Spacing.md) {
            Menu {
                if allChains.isEmpty {
                    Text("No chains in any collection")
                } else {
                    ForEach(groupedChains, id: \.collectionId) { group in
                        Section(header: Text(group.collectionName)) {
                            ForEach(group.chains, id: \.self) { name in
                                Button(name) {
                                    selectedChain = ChainSelection(
                                        collectionId: group.collectionId,
                                        collectionName: group.collectionName,
                                        chainName: name
                                    )
                                }
                            }
                        }
                    }
                }
            } label: {
                HStack {
                    Image(systemName: "link")
                        .font(.system(size: 11))
                        .foregroundStyle(HittTheme.Colors.accent.opacity(0.7))
                    Text(selectedChain.map { "\($0.collectionName) → \($0.chainName)" } ?? "Pick a chain")
                        .font(HittTheme.Typography.mono)
                        .lineLimit(1)
                    Spacer()
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(HittTheme.Colors.textTertiary)
                }
                .padding(.horizontal, HittTheme.Spacing.sm)
                .padding(.vertical, 6)
                .background(HittTheme.Colors.background.opacity(0.5))
                .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)

            Spacer()

            Button {
                guard let s = selectedChain else { return }
                Task { await appVM.runChain(collectionId: s.collectionId, chainName: s.chainName) }
            } label: {
                HStack {
                    if appVM.isRunningChain {
                        ProgressView().scaleEffect(0.5).frame(width: 12, height: 12)
                    } else {
                        Image(systemName: "play.fill").font(.system(size: 10))
                    }
                    Text("Run").font(HittTheme.Typography.bodyMedium)
                }
                .frame(width: 80, height: 28)
            }
            .buttonStyle(.primary)
            .disabled(selectedChain == nil || appVM.isRunningChain)
        }
        .padding(HittTheme.Spacing.md)
    }

    @ViewBuilder private var resultsContent: some View {
        if let r = appVM.lastChainResult {
            VSplitView {
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(r.steps) { step in
                            stepRow(step)
                        }
                    }
                    .padding(HittTheme.Spacing.sm)
                }
                .frame(minHeight: 180)

                if !r.finalVars.isEmpty {
                    finalVarsView(r.finalVars)
                        .frame(minHeight: 140)
                }
            }
        } else {
            EmptyStateView(
                icon: "link",
                title: "Pick a chain and hit Run",
                subtitle: "Chains are defined inside a collection via YAML import."
            )
        }
    }

    private func stepRow(_ step: ChainStepResult) -> some View {
        HStack(alignment: .top, spacing: HittTheme.Spacing.sm) {
            statusIcon(step.status)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text("Step \(step.stepIndex + 1)")
                        .font(HittTheme.Typography.tiny)
                        .tracking(0.5)
                        .foregroundStyle(HittTheme.Colors.textTertiary)
                    if let name = step.requestName {
                        Text(name)
                            .font(HittTheme.Typography.bodyMedium)
                            .foregroundStyle(HittTheme.Colors.textPrimary)
                    }
                    Spacer()
                    if let s = step.httpStatus {
                        StatusBadge(status: s, text: nil)
                    }
                    if let d = step.durationMs {
                        Text("\(d) ms")
                            .font(HittTheme.Typography.monoSmall)
                            .foregroundStyle(HittTheme.Colors.textTertiary)
                    }
                }
                if let err = step.error {
                    Text(err)
                        .font(HittTheme.Typography.caption)
                        .foregroundStyle(HittTheme.Colors.danger)
                }
                if !step.extracted.isEmpty {
                    HStack(spacing: 4) {
                        ForEach(step.extracted.sorted(by: { $0.key < $1.key }), id: \.key) { k, v in
                            HStack(spacing: 3) {
                                Text(k).font(HittTheme.Typography.monoBold)
                                    .foregroundStyle(HittTheme.Colors.accent)
                                Text("=").font(HittTheme.Typography.mono)
                                    .foregroundStyle(HittTheme.Colors.textTertiary)
                                Text(v).font(HittTheme.Typography.mono)
                                    .foregroundStyle(HittTheme.Colors.textSecondary)
                                    .lineLimit(1)
                            }
                            .padding(.horizontal, HittTheme.Spacing.xs)
                            .padding(.vertical, 1)
                            .background(HittTheme.Colors.accent.opacity(0.1))
                            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
                        }
                    }
                }
            }
        }
        .padding(HittTheme.Spacing.sm)
        .background(rowBackground(step.status))
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }

    private func statusIcon(_ status: ChainStepStatus) -> some View {
        let (sys, color): (String, Color) = switch status {
        case .success: ("checkmark.circle.fill", HittTheme.Colors.success)
        case .failed: ("xmark.circle.fill", HittTheme.Colors.danger)
        case .skipped: ("minus.circle.fill", HittTheme.Colors.warning)
        }
        return Image(systemName: sys).font(.system(size: 14)).foregroundStyle(color)
    }

    private func rowBackground(_ status: ChainStepStatus) -> Color {
        switch status {
        case .success: HittTheme.Colors.success.opacity(0.05)
        case .failed: HittTheme.Colors.danger.opacity(0.08)
        case .skipped: HittTheme.Colors.warning.opacity(0.05)
        }
    }

    private func finalVarsView(_ vars: [String: String]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("EXTRACTED VARIABLES")
                .font(HittTheme.Typography.tiny)
                .tracking(0.5)
                .foregroundStyle(HittTheme.Colors.textTertiary)
                .padding(.horizontal, HittTheme.Spacing.md)
                .padding(.top, HittTheme.Spacing.sm)
                .padding(.bottom, HittTheme.Spacing.xs)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(vars.sorted(by: { $0.key < $1.key }), id: \.key) { k, v in
                        HStack(spacing: HittTheme.Spacing.md) {
                            Text(k).font(HittTheme.Typography.monoBold)
                                .foregroundStyle(HittTheme.Colors.accent)
                                .frame(width: 160, alignment: .leading)
                            Text(v).font(HittTheme.Typography.mono)
                                .foregroundStyle(HittTheme.Colors.textPrimary)
                                .textSelection(.enabled)
                            Spacer()
                        }
                        .padding(.horizontal, HittTheme.Spacing.md)
                        .padding(.vertical, 3)
                    }
                }
            }
        }
    }

    private func badge(text: String, color: Color) -> some View {
        Text(text)
            .font(HittTheme.Typography.captionBold)
            .foregroundStyle(color)
            .padding(.horizontal, HittTheme.Spacing.sm)
            .padding(.vertical, 2)
            .background(color.opacity(0.15))
            .clipShape(Capsule())
    }

    // MARK: - Data

    private struct CollectionChainsGroup {
        let collectionId: String
        let collectionName: String
        let chains: [String]
    }

    /// For Phase 3 we list chain names from loaded collections' JSON. If the
    /// collection round-trip kept the chains as JSONValue, we peek at names.
    private var groupedChains: [CollectionChainsGroup] {
        appVM.collections.collections.compactMap { col in
            let names = chainNames(in: col)
            guard !names.isEmpty else { return nil }
            return CollectionChainsGroup(
                collectionId: col.id,
                collectionName: col.name,
                chains: names
            )
        }
    }

    private var allChains: [ChainSelection] {
        groupedChains.flatMap { group in
            group.chains.map {
                ChainSelection(
                    collectionId: group.collectionId,
                    collectionName: group.collectionName,
                    chainName: $0
                )
            }
        }
    }

    /// Extract chain names from the opaque `chains: [JSONValue]` field on HittCollection.
    private func chainNames(in col: HittCollection) -> [String] {
        col.chains.compactMap { value in
            if case .object(let o) = value,
               case .string(let name) = o["name"] ?? .null
            {
                return name
            }
            return nil
        }
    }
}
