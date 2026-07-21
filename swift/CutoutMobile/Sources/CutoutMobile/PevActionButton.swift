import SwiftUI

public struct PevActionButton: View {
    public let title: String
    public let systemImageName: String?
    public let isEnabled: Bool
    public let fillsAvailableWidth: Bool
    public let width: CGFloat?
    public let height: CGFloat
    public let cornerRadius: CGFloat
    public let horizontalPadding: CGFloat
    public let iconSpacing: CGFloat
    public let foregroundEnabled: Color
    public let foregroundDisabled: Color
    public let fillEnabled: Color
    public let fillDisabled: Color
    public let strokeEnabled: Color
    public let strokeDisabled: Color
    public let action: () -> Void

    public init(
        title: String,
        systemImageName: String?,
        isEnabled: Bool,
        fillsAvailableWidth: Bool,
        width: CGFloat?,
        height: CGFloat,
        cornerRadius: CGFloat,
        horizontalPadding: CGFloat,
        iconSpacing: CGFloat,
        foregroundEnabled: Color,
        foregroundDisabled: Color,
        fillEnabled: Color,
        fillDisabled: Color,
        strokeEnabled: Color,
        strokeDisabled: Color,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.systemImageName = systemImageName
        self.isEnabled = isEnabled
        self.fillsAvailableWidth = fillsAvailableWidth
        self.width = width
        self.height = height
        self.cornerRadius = cornerRadius
        self.horizontalPadding = horizontalPadding
        self.iconSpacing = iconSpacing
        self.foregroundEnabled = foregroundEnabled
        self.foregroundDisabled = foregroundDisabled
        self.fillEnabled = fillEnabled
        self.fillDisabled = fillDisabled
        self.strokeEnabled = strokeEnabled
        self.strokeDisabled = strokeDisabled
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            PevActionButtonLabel(
                title: title,
                systemImageName: systemImageName,
                isEnabled: isEnabled,
                fillsAvailableWidth: fillsAvailableWidth,
                width: width,
                height: height,
                cornerRadius: cornerRadius,
                horizontalPadding: horizontalPadding,
                iconSpacing: iconSpacing,
                foregroundEnabled: foregroundEnabled,
                foregroundDisabled: foregroundDisabled,
                fillEnabled: fillEnabled,
                fillDisabled: fillDisabled,
                strokeEnabled: strokeEnabled,
                strokeDisabled: strokeDisabled
            )
        }
        .buttonStyle(.plain)
        .frame(minWidth: 44, minHeight: 44)
        .disabled(!isEnabled)
        .accessibilityLabel(title)
    }
}

public struct PevActionButtonLabel: View {
    @Environment(\.colorSchemeContrast) private var colorSchemeContrast

    public let title: String
    public let systemImageName: String?
    public let isEnabled: Bool
    public let fillsAvailableWidth: Bool
    public let width: CGFloat?
    public let height: CGFloat
    public let cornerRadius: CGFloat
    public let horizontalPadding: CGFloat
    public let iconSpacing: CGFloat
    public let foregroundEnabled: Color
    public let foregroundDisabled: Color
    public let fillEnabled: Color
    public let fillDisabled: Color
    public let strokeEnabled: Color
    public let strokeDisabled: Color

    var hitWidth: CGFloat? {
        width.map { max($0, 44) }
    }

    var hitHeight: CGFloat {
        max(height, 44)
    }

    public init(
        title: String,
        systemImageName: String?,
        isEnabled: Bool,
        fillsAvailableWidth: Bool,
        width: CGFloat?,
        height: CGFloat,
        cornerRadius: CGFloat,
        horizontalPadding: CGFloat,
        iconSpacing: CGFloat,
        foregroundEnabled: Color,
        foregroundDisabled: Color,
        fillEnabled: Color,
        fillDisabled: Color,
        strokeEnabled: Color,
        strokeDisabled: Color
    ) {
        self.title = title
        self.systemImageName = systemImageName
        self.isEnabled = isEnabled
        self.fillsAvailableWidth = fillsAvailableWidth
        self.width = width
        self.height = height
        self.cornerRadius = cornerRadius
        self.horizontalPadding = horizontalPadding
        self.iconSpacing = iconSpacing
        self.foregroundEnabled = foregroundEnabled
        self.foregroundDisabled = foregroundDisabled
        self.fillEnabled = fillEnabled
        self.fillDisabled = fillDisabled
        self.strokeEnabled = strokeEnabled
        self.strokeDisabled = strokeDisabled
    }

    public var body: some View {
        HStack(spacing: systemImageName == nil ? 0 : iconSpacing) {
            if let systemImageName {
                Image(systemName: systemImageName)
                    .font(.callout.weight(.bold))
                    .accessibilityHidden(true)
            }
            Text(title)
                .font(.callout.weight(.bold))
        }
        .foregroundStyle(isEnabled ? foregroundEnabled : foregroundDisabled)
        .padding(.horizontal, horizontalPadding)
        .frame(maxWidth: fillsAvailableWidth ? .infinity : nil)
        .frame(minWidth: hitWidth)
        .frame(minHeight: hitHeight)
        .background(
            RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                .fill(isEnabled ? fillEnabled : fillDisabled)
                .overlay(
                    RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                        .stroke(
                            isEnabled ? strokeEnabled : strokeDisabled,
                            lineWidth: pevDashboardResolvedLineWidth(
                                base: 1,
                                contrast: colorSchemeContrast
                            )
                        )
                )
        )
        .contentShape(Rectangle())
    }
}
