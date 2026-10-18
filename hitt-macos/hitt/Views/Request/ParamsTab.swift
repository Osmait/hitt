import SwiftUI

struct ParamsTab: View {
    @Environment(AppViewModel.self) private var appVM

    var body: some View {
        @Bindable var editor = appVM.editor
        KeyValueTable(
            items: $editor.request.params,
            keyPlaceholder: "Param",
            valuePlaceholder: "Value"
        )
    }
}
