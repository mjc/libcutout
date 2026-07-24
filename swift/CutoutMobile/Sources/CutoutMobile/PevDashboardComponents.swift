import SwiftUI
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

private enum PevSystemColors {
    #if os(iOS)
    static let pageBackground = Color(uiColor: .systemBackground)
    static let disabledFill = pageBackground
    static let yellow = Color(uiColor: .systemYellow)
    static let cardFill = Color(uiColor: .secondarySystemBackground)
    static let primaryText = Color(uiColor: .label)
    static let mutedText = Color(uiColor: .label)
    static let nominal = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? .systemGreen
            : UIColor(red: 0.0, green: 0.38, blue: 0.16, alpha: 1)
    })
    #elseif os(macOS)
    static let pageBackground = Color(nsColor: .windowBackgroundColor)
    static let disabledFill = pageBackground
    static let yellow = Color(nsColor: .systemYellow)
    static let cardFill = Color(nsColor: .underPageBackgroundColor)
    static let primaryText = Color(nsColor: .labelColor)
    static let mutedText = Color(nsColor: .labelColor)
    static let nominal = Color(nsColor: .systemGreen)
    #endif
}

public enum PevDashboardColors {
    public static let disabledFill = PevSystemColors.disabledFill
    public static let yellow = PevSystemColors.yellow
    public static let cardFill = PevSystemColors.cardFill
    public static let cardStroke = Color.secondary.opacity(0.35)
    public static let primaryText = PevSystemColors.primaryText
    public static let mutedText = PevSystemColors.mutedText
    public static let nominal = PevSystemColors.nominal
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
    let dashPattern: [CGFloat]

    public init(
        cornerRadius: CGFloat,
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        lineWidth: CGFloat = 1,
        dashPattern: [CGFloat] = []
    ) {
        self.cornerRadius = cornerRadius
        self.fill = fill
        self.stroke = stroke
        self.lineWidth = lineWidth
        self.dashPattern = dashPattern
    }

    public var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(fill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(
                        stroke,
                        style: StrokeStyle(
                            lineWidth: Self.resolvedLineWidth(
                                base: lineWidth,
                                contrast: colorSchemeContrast
                            ),
                            dash: dashPattern
                        )
                    )
            )
    }

    nonisolated static func resolvedLineWidth(
        base: CGFloat,
        contrast: ColorSchemeContrast
    ) -> CGFloat {
        pevDashboardResolvedLineWidth(base: base, contrast: contrast)
    }
}
