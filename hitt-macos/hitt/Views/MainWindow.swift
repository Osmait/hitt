import SwiftUI
import AppKit

/// Top-level island layout: header + sidebar + main content, floating on a sea background.
struct MainWindow: View {
    @Environment(AppViewModel.self) private var appVM
    @Environment(ThemeManager.self) private var theme

    private enum Island {
        static let gap: CGFloat = 8
        static let radius: CGFloat = HittTheme.Radius.island
        static let outerPadding: CGFloat = 8
        static let trafficLightsWidth: CGFloat = 56
    }

    var body: some View {
        let _ = theme.activeThemeName  // Trigger re-render on theme switch

        ZStack {
            HittTheme.Colors.background.ignoresSafeArea()

            VStack(spacing: Island.gap) {
                header
                HStack(spacing: Island.gap) {
                    if appVM.isSidebarVisible {
                        SidebarView()
                            .frame(width: HittTheme.Size.sidebarWidth)
                            .background(HittTheme.Colors.surface)
                            .clipShape(RoundedRectangle(cornerRadius: Island.radius))
                            .transition(.move(edge: .leading).combined(with: .opacity))
                    }

                    mainContent
                }
                .animation(HittTheme.Animations.quick, value: appVM.isSidebarVisible)
            }
            .padding(.horizontal, Island.outerPadding)
            .padding(.bottom, Island.outerPadding)
            .ignoresSafeArea()

            if let message = appVM.toastMessage {
                VStack {
                    Spacer()
                    ToastNotification(message: message, isError: appVM.toastIsError)
                        .padding(.bottom, HittTheme.Spacing.xl)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                .animation(HittTheme.Animations.spring, value: appVM.toastMessage)
            }
        }
        .frame(minWidth: 900, minHeight: 600)
        .background(WindowAccessor { window in
            window.titlebarAppearsTransparent = true
            window.titleVisibility = .hidden
            window.styleMask.insert(.fullSizeContentView)
        })
        .onAppear { appVM.onAppear() }
        .sheet(isPresented: Binding(
            get: { appVM.isShowingLoadTest },
            set: { appVM.isShowingLoadTest = $0 }
        )) {
            LoadTestModal().environment(appVM)
        }
        .sheet(isPresented: Binding(
            get: { appVM.isShowingDiff },
            set: { appVM.isShowingDiff = $0 }
        )) {
            DiffModal().environment(appVM)
        }
        .sheet(isPresented: Binding(
            get: { appVM.isShowingChainRunner },
            set: { appVM.isShowingChainRunner = $0 }
        )) {
            ChainRunnerModal().environment(appVM)
        }
        .sheet(isPresented: Binding(
            get: { appVM.isShowingWebSocket },
            set: { appVM.isShowingWebSocket = $0 }
        )) {
            WebSocketModal().environment(appVM)
        }
        .sheet(isPresented: Binding(
            get: { appVM.isShowingSse },
            set: { appVM.isShowingSse = $0 }
        )) {
            SseModal().environment(appVM)
        }
        .sheet(isPresented: Binding(
            get: { appVM.isShowingHistory },
            set: { appVM.isShowingHistory = $0 }
        )) {
            HistoryModal().environment(appVM)
        }
        .sheet(isPresented: Binding(
            get: { appVM.isShowingEnvironmentEditor },
            set: { appVM.isShowingEnvironmentEditor = $0 }
        )) {
            EnvironmentEditorSheet().environment(appVM)
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: HittTheme.Spacing.sm) {
            Spacer().frame(width: Island.trafficLightsWidth)

            Button {
                withAnimation(HittTheme.Animations.quick) {
                    appVM.isSidebarVisible.toggle()
                }
            } label: {
                Image(systemName: "sidebar.leading")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(
                        appVM.isSidebarVisible
                            ? HittTheme.Colors.accent
                            : HittTheme.Colors.textTertiary
                    )
            }
            .buttonStyle(.hoverIcon)
            .keyboardShortcut("s", modifiers: [.command, .control])
            .accessibilityLabel("Toggle Sidebar")

            HittTheme.Colors.border.frame(width: 1, height: 16).opacity(0.5)

            Text("hitt")
                .font(HittTheme.Typography.headline)
                .foregroundStyle(HittTheme.Colors.textPrimary)

            // Active request name (if any)
            if !appVM.editor.request.name.isEmpty && appVM.editor.request.name != "Untitled" {
                HittTheme.Colors.border.frame(width: 1, height: 16).opacity(0.5)
                Text(appVM.editor.request.name)
                    .font(HittTheme.Typography.caption)
                    .foregroundStyle(HittTheme.Colors.textSecondary)
                    .lineLimit(1)
            }

            Spacer()

            Button {
                appVM.isShowingLoadTest = true
            } label: {
                Image(systemName: "bolt.fill")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.accent.opacity(0.7))
            }
            .buttonStyle(.hoverIcon)
            .help("Load test")

            Button {
                appVM.isShowingDiff = true
            } label: {
                Image(systemName: "doc.on.doc")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.accent.opacity(0.7))
            }
            .buttonStyle(.hoverIcon)
            .help("Diff responses")

            Button {
                appVM.isShowingChainRunner = true
            } label: {
                Image(systemName: "link")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.accent.opacity(0.7))
            }
            .buttonStyle(.hoverIcon)
            .help("Run chain")

            Button {
                appVM.isShowingWebSocket = true
            } label: {
                Image(systemName: "arrow.left.arrow.right.circle")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.accent.opacity(0.7))
            }
            .buttonStyle(.hoverIcon)
            .help("WebSocket")

            Button {
                appVM.isShowingSse = true
            } label: {
                Image(systemName: "dot.radiowaves.up.forward")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.accent.opacity(0.7))
            }
            .buttonStyle(.hoverIcon)
            .help("Server-Sent Events")

            Button {
                appVM.isShowingHistory = true
            } label: {
                Image(systemName: "clock.arrow.circlepath")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.accent.opacity(0.7))
            }
            .buttonStyle(.hoverIcon)
            .help("History")

            HittTheme.Colors.border.frame(width: 1, height: 16).opacity(0.5)

            themeMenu
        }
        .padding(.horizontal, HittTheme.Spacing.lg)
        .padding(.top, 4)
        .padding(.bottom, HittTheme.Spacing.sm)
        .frame(height: HittTheme.Size.headerHeight + 4)
        .background(HittTheme.Colors.surface)
        .clipShape(
            UnevenRoundedRectangle(
                topLeadingRadius: 0,
                bottomLeadingRadius: Island.radius,
                bottomTrailingRadius: Island.radius,
                topTrailingRadius: 0
            )
        )
    }

    @ViewBuilder private var themeMenu: some View {
        @Bindable var tm = theme
        Menu {
            ForEach(ThemeName.allCases) { name in
                Button(name.rawValue) { tm.activeThemeName = name }
            }
        } label: {
            Image(systemName: "paintpalette")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(HittTheme.Colors.accent.opacity(0.7))
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    // MARK: - Main content

    private var mainContent: some View {
        VSplitView {
            RequestEditorView()
                .frame(minHeight: 180)
                .background(HittTheme.Colors.surface)
                .clipShape(RoundedRectangle(cornerRadius: Island.radius))

            ResponseView()
                .frame(minHeight: 200)
                .background(HittTheme.Colors.surface)
                .clipShape(RoundedRectangle(cornerRadius: Island.radius))
        }
    }
}

/// NSWindow accessor for tweaking the titlebar (hidden + transparent).
struct WindowAccessor: NSViewRepresentable {
    let callback: (NSWindow) -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            if let window = view.window {
                callback(window)
            }
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}
