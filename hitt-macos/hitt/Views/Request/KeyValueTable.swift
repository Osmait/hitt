import SwiftUI

/// Editable grid of KeyValuePair rows — used by Headers and Params tabs.
///
/// Layout:
///   - Key column is narrow (typical header/param names like "Accept" or
///     "limit" are short). Flexes 180–360 pt.
///   - Value column takes the remaining width up to a sensible max.
///   - A full-height vertical divider visually separates the two columns.
///   - The whole table is wrapped in a ScrollView, so the "Add" button
///     sits at the bottom of the *content* (not glued to the panel bottom)
///     and a near-empty table doesn't show a huge void.
struct KeyValueTable: View {
    @Binding var items: [KeyValuePair]
    var keyPlaceholder: String = "Key"
    var valuePlaceholder: String = "Value"

    // Column sizing.
    private let keyMinWidth: CGFloat = 180
    private let keyMaxWidth: CGFloat = 360
    private let checkboxWidth: CGFloat = 22
    private let deleteWidth: CGFloat = 22
    private let cellVPadding: CGFloat = 5
    private let cellHPadding: CGFloat = HittTheme.Spacing.sm

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                headerRow
                Divider().background(HittTheme.Colors.border.opacity(0.6))

                ForEach($items) { $item in
                    RowView(
                        item: $item,
                        keyPlaceholder: keyPlaceholder,
                        valuePlaceholder: valuePlaceholder,
                        keyMinWidth: keyMinWidth,
                        keyMaxWidth: keyMaxWidth,
                        checkboxWidth: checkboxWidth,
                        deleteWidth: deleteWidth,
                        cellVPadding: cellVPadding,
                        cellHPadding: cellHPadding,
                        onDelete: {
                            items.removeAll { $0.id == item.id }
                        }
                    )
                    Divider().background(HittTheme.Colors.borderSubtle.opacity(0.5))
                }

                addRowButton
            }
        }
        .scrollContentBackground(.hidden)
    }

    // MARK: - Header row

    private var headerRow: some View {
        HStack(spacing: 0) {
            Color.clear.frame(width: checkboxWidth + cellHPadding)

            Text(keyPlaceholder.uppercased())
                .font(HittTheme.Typography.tiny)
                .tracking(0.5)
                .foregroundStyle(HittTheme.Colors.textTertiary)
                .frame(minWidth: keyMinWidth, idealWidth: keyMinWidth + 40, maxWidth: keyMaxWidth, alignment: .leading)
                .padding(.horizontal, cellHPadding)

            Rectangle()
                .fill(HittTheme.Colors.borderSubtle)
                .frame(width: 1)
                .padding(.vertical, 2)

            Text(valuePlaceholder.uppercased())
                .font(HittTheme.Typography.tiny)
                .tracking(0.5)
                .foregroundStyle(HittTheme.Colors.textTertiary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, cellHPadding)

            Color.clear.frame(width: deleteWidth + cellHPadding)
        }
        .padding(.vertical, 6)
        .background(HittTheme.Colors.surfaceElevated.opacity(0.3))
    }

    // MARK: - Add row

    private var addRowButton: some View {
        HStack {
            Button {
                items.append(KeyValuePair())
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "plus.circle")
                        .font(.system(size: 11))
                    Text("Add")
                        .font(HittTheme.Typography.bodyMedium)
                }
                .foregroundStyle(HittTheme.Colors.textSecondary)
                .padding(.horizontal, cellHPadding + checkboxWidth)
                .padding(.vertical, 6)
            }
            .buttonStyle(.hover)
            Spacer()
        }
        .padding(.top, 2)
    }
}

// MARK: - Row

private struct RowView: View {
    @Binding var item: KeyValuePair
    let keyPlaceholder: String
    let valuePlaceholder: String
    let keyMinWidth: CGFloat
    let keyMaxWidth: CGFloat
    let checkboxWidth: CGFloat
    let deleteWidth: CGFloat
    let cellVPadding: CGFloat
    let cellHPadding: CGFloat
    let onDelete: () -> Void

    @State private var isHovered: Bool = false

    var body: some View {
        HStack(spacing: 0) {
            Toggle(isOn: $item.enabled) { EmptyView() }
                .toggleStyle(.checkbox)
                .labelsHidden()
                .frame(width: checkboxWidth)
                .padding(.leading, cellHPadding)

            TextField(keyPlaceholder, text: $item.key)
                .textFieldStyle(.plain)
                .font(HittTheme.Typography.mono)
                .foregroundStyle(keyColor)
                .frame(minWidth: keyMinWidth, idealWidth: keyMinWidth + 40, maxWidth: keyMaxWidth, alignment: .leading)
                .padding(.horizontal, cellHPadding)
                .padding(.vertical, cellVPadding)

            Rectangle()
                .fill(HittTheme.Colors.borderSubtle)
                .frame(width: 1)

            TextField(valuePlaceholder, text: $item.value)
                .textFieldStyle(.plain)
                .font(HittTheme.Typography.mono)
                .foregroundStyle(valueColor)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, cellHPadding)
                .padding(.vertical, cellVPadding)

            Button(action: onDelete) {
                Image(systemName: "xmark")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(HittTheme.Colors.textTertiary)
                    .frame(width: deleteWidth, height: 18)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(isHovered ? 1 : 0.35)
            .padding(.trailing, cellHPadding)
        }
        .background(rowBackground)
        .onHover { isHovered = $0 }
    }

    private var keyColor: Color {
        item.enabled ? HittTheme.Colors.accent.opacity(0.9) : HittTheme.Colors.textTertiary
    }

    private var valueColor: Color {
        item.enabled ? HittTheme.Colors.textPrimary : HittTheme.Colors.textTertiary
    }

    private var rowBackground: Color {
        if isHovered {
            HittTheme.Colors.surfaceHover.opacity(0.35)
        } else {
            Color.clear
        }
    }
}
