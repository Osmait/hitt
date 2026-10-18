import SwiftUI

/// Displays assertion results for the last response.
struct ResponseTestsView: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        if let resp = appVM.response.lastResponse, !resp.assertionResults.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                summaryBar(resp.assertionResults)
                Divider().background(HittTheme.Colors.borderSubtle)
                ScrollView {
                    LazyVStack(spacing: 4) {
                        ForEach(resp.assertionResults) { result in
                            resultRow(result)
                        }
                    }
                    .padding(HittTheme.Spacing.sm)
                }
            }
        } else if appVM.response.lastResponse != nil {
            EmptyStateView(
                icon: "checkmark.seal",
                title: "No tests to run",
                subtitle: "Add assertions in the Tests tab of the request editor."
            )
        } else {
            EmptyStateView(icon: "checkmark.seal", title: "No response yet")
        }
    }

    private func summaryBar(_ results: [AssertionResultModel]) -> some View {
        let passed = results.filter(\.passed).count
        let failed = results.count - passed
        let allPass = failed == 0
        return HStack {
            Image(systemName: allPass ? "checkmark.seal.fill" : "xmark.seal.fill")
                .font(.system(size: 14))
                .foregroundStyle(allPass ? HittTheme.Colors.success : HittTheme.Colors.danger)
            Text("\(passed) of \(results.count) passed")
                .font(HittTheme.Typography.bodyMedium)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            Spacer()
        }
        .padding(HittTheme.Spacing.md)
        .background(
            (allPass ? HittTheme.Colors.success : HittTheme.Colors.danger).opacity(0.08)
        )
    }

    private func resultRow(_ r: AssertionResultModel) -> some View {
        HStack(alignment: .top, spacing: HittTheme.Spacing.sm) {
            Image(systemName: r.passed ? "checkmark.circle.fill" : "xmark.circle.fill")
                .font(.system(size: 12))
                .foregroundStyle(r.passed ? HittTheme.Colors.success : HittTheme.Colors.danger)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 2) {
                Text(r.assertion.kind.displayLabel)
                    .font(HittTheme.Typography.monoBold)
                    .foregroundStyle(HittTheme.Colors.textPrimary)
                if !r.message.isEmpty {
                    Text(r.message)
                        .font(HittTheme.Typography.caption)
                        .foregroundStyle(HittTheme.Colors.textSecondary)
                }
                if let actual = r.actualValue, !actual.isEmpty {
                    Text("actual: \(actual)")
                        .font(HittTheme.Typography.monoSmall)
                        .foregroundStyle(HittTheme.Colors.textTertiary)
                        .textSelection(.enabled)
                }
            }
            Spacer()
        }
        .padding(HittTheme.Spacing.sm)
        .background(
            (r.passed ? HittTheme.Colors.success : HittTheme.Colors.danger).opacity(0.06)
        )
        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
    }
}
