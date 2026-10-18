import SwiftUI

struct RequestEditorView: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        VStack(spacing: 0) {
            UrlBar()
            Divider().background(HittTheme.Colors.borderSubtle)
            TabBar()
            Divider().background(HittTheme.Colors.borderSubtle)
            tabContent
        }
    }

    @ViewBuilder private var tabContent: some View {
        switch appVM.editor.selectedTab {
        case .params: ParamsTab()
        case .headers: HeadersTab()
        case .body: BodyTab()
        case .auth: AuthTab()
        case .tests: AssertionsTab()
        }
    }
}

// MARK: - TabBar (extracted to keep the type-checker fast)

private struct TabBar: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        @Bindable var editor = appVM.editor
        HStack(spacing: 2) {
            ForEach(RequestEditorViewModel.EditorTab.allCases) { tab in
                TabButton(tab: tab, editor: editor)
            }
            Spacer()
        }
        .padding(.horizontal, HittTheme.Spacing.md)
        .padding(.top, HittTheme.Spacing.xs)
        .background(HittTheme.Colors.surface)
    }
}

private struct TabButton: View {
    let tab: RequestEditorViewModel.EditorTab
    let editor: RequestEditorViewModel

    var body: some View {
        Button {
            editor.selectedTab = tab
        } label: {
            HStack(spacing: 6) {
                label
                annotation
            }
            .padding(.horizontal, HittTheme.Spacing.md)
            .padding(.vertical, HittTheme.Spacing.xs)
            .background(background)
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
            .overlay(alignment: .bottom) { underline }
            .contentShape(Rectangle())
        }
        .buttonStyle(.hover)
    }

    private var isSelected: Bool { editor.selectedTab == tab }

    private var label: some View {
        Text(tab.rawValue)
            .font(HittTheme.Typography.captionBold)
            .foregroundStyle(isSelected ? HittTheme.Colors.textPrimary : HittTheme.Colors.textTertiary)
    }

    @ViewBuilder private var annotation: some View {
        switch tab {
        case .params where editor.paramsCount > 0:
            BadgePill(count: editor.paramsCount)
        case .headers where editor.headersCount > 0:
            BadgePill(count: editor.headersCount)
        case .auth where editor.authTag != "none":
            Circle().fill(HittTheme.Colors.accent).frame(width: 6, height: 6)
        case .body where editor.bodyTag != "none":
            Circle().fill(HittTheme.Colors.accent).frame(width: 6, height: 6)
        case .tests where editor.assertionsCount > 0:
            BadgePill(count: editor.assertionsCount)
        default:
            EmptyView()
        }
    }

    @ViewBuilder private var background: some View {
        if isSelected {
            HittTheme.Colors.surfaceElevated
        } else {
            Color.clear
        }
    }

    @ViewBuilder private var underline: some View {
        if isSelected {
            HittTheme.Colors.accent.frame(height: 2)
        }
    }
}

private struct BadgePill: View {
    let count: Int

    var body: some View {
        Text("\(count)")
            .font(HittTheme.Typography.tiny)
            .monospaced()
            .foregroundStyle(HittTheme.Colors.textSecondary)
            .padding(.horizontal, 4)
            .padding(.vertical, 1)
            .background(HittTheme.Colors.surfaceHover.opacity(0.5))
            .clipShape(Capsule())
    }
}
