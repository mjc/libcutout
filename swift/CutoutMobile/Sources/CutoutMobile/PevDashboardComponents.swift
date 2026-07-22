import SwiftUI
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

private enum PevSystemColors {
    #if os(iOS)
    static let cardFill = Color(uiColor: .secondarySystemBackground)
    static let primaryText = Color(uiColor: .label)
    static let mutedText = Color(uiColor: .label)
    #elseif os(macOS)
    static let cardFill = Color(nsColor: .underPageBackgroundColor)
    static let primaryText = Color(nsColor: .labelColor)
    static let mutedText = Color(nsColor: .labelColor)
    #endif
}

public enum PevDashboardColors {
    public static let cardFill = PevSystemColors.cardFill
    public static let cardStroke = Color.secondary.opacity(0.35)
    public static let primaryText = PevSystemColors.primaryText
    public static let mutedText = PevSystemColors.mutedText
}

public func pevDashboardAccessibilityValue(_ parts: [String]) -> String {
    parts.filter { !$0.isEmpty }.formatted(.list(type: .and))
}

nonisolated func pevDashboardResolvedLineWidth(
    base: CGFloat,
    contrast: ColorSchemeContrast
) -> CGFloat {
    contrast == .increased ? max(2, base * 1.5) : base
}

public struct PevDashboardCardBackground: View {
    @Environment(\.colorSchemeContrast) private var colorSchemeContrast

    let cornerRadius: CGFloat
    let fill: Color
    let stroke: Color
    let lineWidth: CGFloat

    public init(
        cornerRadius: CGFloat,
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        lineWidth: CGFloat = 1
    ) {
        self.cornerRadius = cornerRadius
        self.fill = fill
        self.stroke = stroke
        self.lineWidth = lineWidth
    }

    public var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(fill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(stroke, lineWidth: Self.resolvedLineWidth(
                        base: lineWidth,
                        contrast: colorSchemeContrast
                    ))
            )
    }

    nonisolated static func resolvedLineWidth(
        base: CGFloat,
        contrast: ColorSchemeContrast
    ) -> CGFloat {
        pevDashboardResolvedLineWidth(base: base, contrast: contrast)
    }
}
