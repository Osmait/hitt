import SwiftUI

struct HoverButtonStyle: ButtonStyle {
    @State private var isHovered = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(
                isHovered
                    ? HittTheme.Colors.surfaceHover.opacity(0.5)
                    : Color.clear
            )
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
            .scaleEffect(configuration.isPressed ? 0.96 : 1.0)
            .opacity(configuration.isPressed ? 0.8 : 1.0)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
            .animation(.easeOut(duration: 0.15), value: isHovered)
            .onHover { isHovered = $0 }
    }
}

struct HoverIconButtonStyle: ButtonStyle {
    @State private var isHovered = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(4)
            .background(
                isHovered
                    ? HittTheme.Colors.surfaceHover.opacity(0.6)
                    : Color.clear
            )
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.small))
            .scaleEffect(configuration.isPressed ? 0.9 : isHovered ? 1.05 : 1.0)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
            .animation(.easeOut(duration: 0.15), value: isHovered)
            .onHover { isHovered = $0 }
    }
}

struct PrimaryButtonStyle: ButtonStyle {
    @State private var isHovered = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(HittTheme.Typography.bodyMedium)
            .foregroundStyle(Color.black)
            .padding(.horizontal, HittTheme.Spacing.md)
            .padding(.vertical, HittTheme.Spacing.xs)
            .background(
                isHovered
                    ? HittTheme.Colors.accentHover
                    : HittTheme.Colors.accent
            )
            .clipShape(RoundedRectangle(cornerRadius: HittTheme.Radius.medium))
            .scaleEffect(configuration.isPressed ? 0.97 : 1.0)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
            .animation(.easeOut(duration: 0.15), value: isHovered)
            .onHover { isHovered = $0 }
    }
}

extension ButtonStyle where Self == HoverButtonStyle {
    static var hover: HoverButtonStyle { HoverButtonStyle() }
}

extension ButtonStyle where Self == HoverIconButtonStyle {
    static var hoverIcon: HoverIconButtonStyle { HoverIconButtonStyle() }
}

extension ButtonStyle where Self == PrimaryButtonStyle {
    static var primary: PrimaryButtonStyle { PrimaryButtonStyle() }
}
