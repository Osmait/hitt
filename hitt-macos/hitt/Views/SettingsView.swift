import SwiftUI

/// macOS Settings scene (opens via Cmd+,).
struct SettingsView: View {
    var body: some View {
        TabView {
            AppearanceSettingsView()
                .tabItem { Label("Appearance", systemImage: "paintpalette") }
            GeneralSettingsView()
                .tabItem { Label("General", systemImage: "gearshape") }
            AboutSettingsView()
                .tabItem { Label("About", systemImage: "info.circle") }
        }
        .frame(width: 480, height: 360)
    }
}

// MARK: - Appearance

private struct AppearanceSettingsView: View {
    @Environment(ThemeManager.self) private var theme

    var body: some View {
        @Bindable var theme = theme
        Form {
            Section("Theme") {
                Picker("Palette", selection: $theme.activeThemeName) {
                    ForEach(ThemeName.allCases) { name in
                        Text(name.rawValue).tag(name)
                    }
                }
                .pickerStyle(.menu)
            }

            Section("Preview") {
                HStack(spacing: HittTheme.Spacing.sm) {
                    swatch("BG", HittTheme.Colors.background)
                    swatch("Surf", HittTheme.Colors.surface)
                    swatch("Acc", HittTheme.Colors.accent)
                    swatch("OK", HittTheme.Colors.success)
                    swatch("Warn", HittTheme.Colors.warning)
                    swatch("Err", HittTheme.Colors.danger)
                }
                HStack(spacing: HittTheme.Spacing.xs) {
                    ForEach(HttpMethod.allCases) { m in
                        MethodBadge(method: m, style: .filled)
                    }
                }
            }
        }
        .formStyle(.grouped)
    }

    private func swatch(_ label: String, _ color: Color) -> some View {
        VStack(spacing: 2) {
            RoundedRectangle(cornerRadius: 6)
                .fill(color)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(HittTheme.Colors.border, lineWidth: 0.5))
                .frame(width: 44, height: 28)
            Text(label).font(HittTheme.Typography.tiny)
                .foregroundStyle(HittTheme.Colors.textTertiary)
        }
    }
}

// MARK: - General

private struct GeneralSettingsView: View {
    @AppStorage("historyLimit") private var historyLimit: Int = 100
    @AppStorage("requestTimeoutMs") private var timeoutMs: Int = 30000
    @AppStorage("autoScrollResponses") private var autoScroll: Bool = true

    var body: some View {
        Form {
            Section("History") {
                HStack {
                    Text("Max entries")
                    Spacer()
                    TextField("", value: $historyLimit, formatter: NumberFormatter())
                        .frame(width: 90)
                        .multilineTextAlignment(.trailing)
                }
                Text("How many past requests to keep in memory.")
                    .font(HittTheme.Typography.caption)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            Section("Network") {
                HStack {
                    Text("Request timeout (ms)")
                    Spacer()
                    TextField("", value: $timeoutMs, formatter: NumberFormatter())
                        .frame(width: 90)
                        .multilineTextAlignment(.trailing)
                }
            }
            Section("Response panel") {
                Toggle("Auto-scroll streams to latest", isOn: $autoScroll)
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: - About

private struct AboutSettingsView: View {
    private var version: String {
        (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "0.1.0"
    }

    var body: some View {
        VStack(spacing: HittTheme.Spacing.lg) {
            Spacer().frame(height: HittTheme.Spacing.md)
            Image(systemName: "paperplane.circle.fill")
                .resizable()
                .frame(width: 72, height: 72)
                .foregroundStyle(HittTheme.Colors.accent)
            VStack(spacing: 4) {
                Text("hitt")
                    .font(HittTheme.Typography.title)
                    .foregroundStyle(HittTheme.Colors.textPrimary)
                Text("Fast HTTP client for macOS")
                    .font(HittTheme.Typography.caption)
                    .foregroundStyle(HittTheme.Colors.textSecondary)
                Text("Version \(version)")
                    .font(HittTheme.Typography.monoSmall)
                    .foregroundStyle(HittTheme.Colors.textTertiary)
            }
            VStack(alignment: .leading, spacing: HittTheme.Spacing.xs) {
                Text("Built with Rust core (`hitt-core`) bridged to SwiftUI via UniFFI.")
                    .font(HittTheme.Typography.caption)
                    .foregroundStyle(HittTheme.Colors.textSecondary)
            }
            .padding(HittTheme.Spacing.md)
            .background(HittTheme.Colors.surfaceElevated.opacity(0.5))
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.medium))
            Spacer()
        }
        .padding(HittTheme.Spacing.lg)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
