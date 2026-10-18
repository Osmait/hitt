import SwiftUI

struct CollectionRow: View {
    @Environment(AppViewModel.self) private var appVM
    let collection: HittCollection
    @State private var isExpanded: Bool = true

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(HittTheme.Animations.quick) { isExpanded.toggle() }
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(HittTheme.Colors.textTertiary)
                        .frame(width: 10)
                    Image(systemName: "folder.fill")
                        .font(.system(size: 11))
                        .foregroundStyle(HittTheme.Colors.accent.opacity(0.8))
                    Text(collection.name)
                        .font(HittTheme.Typography.bodyMedium)
                        .foregroundStyle(HittTheme.Colors.textPrimary)
                        .lineLimit(1)
                    Spacer()
                    Text("\(collection.allRequests.count)")
                        .font(HittTheme.Typography.tiny)
                        .monospaced()
                        .foregroundStyle(HittTheme.Colors.textTertiary)
                }
                .padding(.horizontal, HittTheme.Spacing.sm)
                .padding(.vertical, 5)
                .contentShape(Rectangle())
            }
            .buttonStyle(.hover)
            .contextMenu {
                Button {
                    appVM.exportCollectionAsMarkdown(collection)
                } label: {
                    Label("Export as Markdown…", systemImage: "doc.text")
                }
                Button {
                    appVM.exportCollectionAsPostman(collection)
                } label: {
                    Label("Export as Postman…", systemImage: "square.and.arrow.up")
                }
                Divider()
                Button(role: .destructive) {
                    Task { await appVM.deleteCollection(collection) }
                } label: {
                    Label("Delete Collection", systemImage: "trash")
                }
            }

            if isExpanded {
                ForEach(collection.allRequests) { req in
                    RequestRow(request: req)
                        .padding(.leading, 16)
                }
            }
        }
    }
}
