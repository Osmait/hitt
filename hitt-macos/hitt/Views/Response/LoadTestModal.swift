import SwiftUI

struct LoadTestModal: View {
    @Environment(AppViewModel.self) private var appVM
    @Environment(\.dismiss) private var dismiss

    @State private var total: UInt32 = 100
    @State private var concurrency: UInt32 = 10
    @State private var timeoutSec: UInt32 = 30

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().background(HittTheme.Colors.borderSubtle)
            ScrollView {
                VStack(alignment: .leading, spacing: HittTheme.Spacing.md) {
                    configSection
                    if appVM.isRunningLoadTest {
                        HStack(spacing: 8) {
                            ProgressView().scaleEffect(0.7)
                            Text("Running \(total) requests at concurrency \(concurrency)…")
                                .font(HittTheme.Typography.caption)
                                .foregroundStyle(HittTheme.Colors.textSecondary)
                        }
                        .padding(HittTheme.Spacing.md)
                    }
                    if let result = appVM.lastLoadTestResult {
                        resultSection(result)
                    }
                }
                .padding(HittTheme.Spacing.lg)
            }
        }
        .frame(minWidth: 520, minHeight: 460)
        .background(HittTheme.Colors.surface)
    }

    private var header: some View {
        HStack {
            Text("Load Test").font(HittTheme.Typography.title)
                .foregroundStyle(HittTheme.Colors.textPrimary)
            Spacer()
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 16))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            .buttonStyle(.plain)
        }
        .padding(HittTheme.Spacing.lg)
    }

    @ViewBuilder private var configSection: some View {
        Text(appVM.editor.request.url.isEmpty ? "No URL set" : appVM.editor.request.url)
            .font(HittTheme.Typography.mono)
            .foregroundStyle(HittTheme.Colors.textSecondary)
            .lineLimit(1)

        HStack(spacing: HittTheme.Spacing.md) {
            numberField(label: "Requests", value: $total, range: 1...100_000)
            numberField(label: "Concurrency", value: $concurrency, range: 1...1000)
            numberField(label: "Timeout (s)", value: $timeoutSec, range: 1...600)
        }

        Button {
            Task { await appVM.runLoadTest(total: total, concurrency: concurrency, timeoutMs: UInt64(timeoutSec) * 1000) }
        } label: {
            HStack {
                Image(systemName: "bolt.fill")
                Text("Run load test").font(HittTheme.Typography.bodyMedium)
            }
            .frame(width: 180, height: 32)
        }
        .buttonStyle(.primary)
        .disabled(appVM.isRunningLoadTest || appVM.editor.request.url.isEmpty)
    }

    private func resultSection(_ r: LoadTestResult) -> some View {
        VStack(alignment: .leading, spacing: HittTheme.Spacing.sm) {
            Divider().background(HittTheme.Colors.borderSubtle)
            Text("Results")
                .font(HittTheme.Typography.headline)
                .foregroundStyle(HittTheme.Colors.textPrimary)

            HStack(spacing: HittTheme.Spacing.md) {
                stat("Total", "\(r.totalRequests)", color: HittTheme.Colors.textPrimary)
                stat("OK", "\(r.successful)", color: HittTheme.Colors.success)
                stat("Failed", "\(r.failed)", color: HittTheme.Colors.danger)
                stat("RPS", String(format: "%.1f", r.rps), color: HittTheme.Colors.accent)
            }

            Text("Latency (ms)")
                .font(HittTheme.Typography.captionBold)
                .tracking(0.5)
                .foregroundStyle(HittTheme.Colors.textTertiary)
                .padding(.top, HittTheme.Spacing.sm)

            HStack(spacing: HittTheme.Spacing.md) {
                stat("min", "\(r.latency.min)")
                stat("med", "\(r.latency.median)")
                stat("mean", "\(r.latency.mean)")
                stat("p90", "\(r.latency.p90)")
                stat("p95", "\(r.latency.p95)")
                stat("p99", "\(r.latency.p99)")
                stat("max", "\(r.latency.max)")
            }

            if !r.statusDistribution.isEmpty {
                Text("Status distribution")
                    .font(HittTheme.Typography.captionBold)
                    .tracking(0.5)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
                    .padding(.top, HittTheme.Spacing.sm)
                HStack(spacing: HittTheme.Spacing.sm) {
                    ForEach(r.statusDistribution.sorted(by: { $0.key < $1.key }), id: \.key) { entry in
                        HStack(spacing: 4) {
                            Text(entry.key).font(HittTheme.Typography.monoBold)
                            Text("× \(entry.value)").font(HittTheme.Typography.caption)
                        }
                        .padding(.horizontal, HittTheme.Spacing.sm)
                        .padding(.vertical, 3)
                        .background(statusColor(entry.key).opacity(0.15))
                        .foregroundStyle(statusColor(entry.key))
                        .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
                    }
                }
            }
        }
    }

    private func stat(_ label: String, _ value: String, color: Color = HittTheme.Colors.textPrimary) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(HittTheme.Typography.tiny)
                .tracking(0.5)
                .foregroundStyle(HittTheme.Colors.textTertiary)
            Text(value)
                .font(HittTheme.Typography.monoBold)
                .foregroundStyle(color)
        }
    }

    private func numberField(label: String, value: Binding<UInt32>, range: ClosedRange<UInt32>) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(HittTheme.Typography.tiny)
                .tracking(0.5)
                .foregroundStyle(HittTheme.Colors.textTertiary)
            TextField("", value: value, formatter: NumberFormatter())
                .textFieldStyle(.roundedBorder)
                .font(HittTheme.Typography.mono)
                .frame(width: 90)
        }
    }

    private func statusColor(_ status: String) -> Color {
        guard let code = Int(status) else { return HittTheme.Colors.textSecondary }
        switch code {
        case 200..<300: return HittTheme.Colors.success
        case 300..<400: return HittTheme.Colors.warning
        case 400..<600: return HittTheme.Colors.danger
        default: return HittTheme.Colors.textSecondary
        }
    }
}
