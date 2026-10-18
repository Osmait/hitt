import SwiftUI

struct HeadersTab: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        @Bindable var editor = appVM.editor
        KeyValueTable(
            items: $editor.request.headers,
            keyPlaceholder: "Header",
            valuePlaceholder: "Value"
        )
    }
}
