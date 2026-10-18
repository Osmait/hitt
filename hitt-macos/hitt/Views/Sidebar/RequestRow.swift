import SwiftUI

struct RequestRow: View {
    @Environment(AppViewModel.self) private var appVM
    let request: Request

    private var isSelected: Bool {
        appVM.collections.selectedRequestId == request.id
    }

    var body: some View {
        Button {
            appVM.selectRequest(request)
        } label: {
            HStack(spacing: HittTheme.Spacing.sm) {
                MethodBadge(method: request.method, style: .text)
                    .frame(width: 52, alignment: .leading)
                Text(request.name.isEmpty ? request.url : request.name)
                    .font(HittTheme.Typography.body)
                    .foregroundStyle(isSelected ? HittTheme.Colors.textPrimary : HittTheme.Colors.textSecondary)
                    .lineLimit(1)
                Spacer()
            }
            .padding(.horizontal, HittTheme.Spacing.sm)
            .padding(.vertical, 4)
            .background(isSelected ? HittTheme.Colors.surfaceElevated : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
