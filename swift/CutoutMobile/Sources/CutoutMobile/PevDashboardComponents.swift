import SwiftUI
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

private enum PevSystemColors {
    #if os(iOS)
    static let cardFill = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 0.067, green: 0.078, blue: 0.106, alpha: 1)
            : UIColor(red: 0.95, green: 0.95, blue: 0.97, alpha: 1)
    })
    static let primaryText = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark ? .white : .black
    })
    static let mutedText = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(white: 0.76, alpha: 1)
            : UIColor(white: 0.35, alpha: 1)
    })
    #elseif os(macOS)
    static let cardFill = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(red: 0.067, green: 0.078, blue: 0.106, alpha: 1)
            : NSColor(red: 0.95, green: 0.95, blue: 0.97, alpha: 1)
    })
    static let primaryText = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? .white : .black
    })
    static let mutedText = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(white: 0.76, alpha: 1)
            : NSColor(white: 0.35, alpha: 1)
    })
    #endif
}

public enum PevDashboardColors {
    public static let cardFill = PevSystemColors.cardFill
    public static let cardStroke = Color.secondary.opacity(0.35)
    public static let primaryText = PevSystemColors.primaryText
    public static let mutedText = PevSystemColors.mutedText
}

func pevDashboardAccessibilityValue(_ parts: [String]) -> String {
    parts.filter { !$0.isEmpty }.joined(separator: ", ")
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
