//
//  hittApp.swift
//  hitt
//
//  Root SwiftUI app — injects AppViewModel + ThemeManager.
//

import SwiftUI

@main
struct hittApp: App {
    @State private var appVM = AppViewModel()

    var body: some Scene {
        WindowGroup {
            MainWindow()
                .environment(appVM)
                .environment(ThemeManager.shared)
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1200, height: 800)

        Settings {
            SettingsView()
                .environment(appVM)
                .environment(ThemeManager.shared)
        }
    }
}
