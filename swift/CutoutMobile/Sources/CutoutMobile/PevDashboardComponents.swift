import SwiftUI

#if os(iOS)
    import UIKit
#elseif os(macOS)
    import AppKit
#endif

private enum PevSystemColors {
    #if os(iOS)
        static let pageBackground = Color(uiColor: .systemBackground)
        static let rideCanvas = Color(
            uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? UIColor(red: 8 / 255, green: 10 / 255, blue: 9 / 255, alpha: 1)
                    : UIColor(red: 250 / 255, green: 249 / 255, blue: 244 / 255, alpha: 1)
            })
        static let rideInk = Color(
            uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? UIColor(red: 248 / 255, green: 247 / 255, blue: 239 / 255, alpha: 1)
                    : UIColor(red: 17 / 255, green: 19 / 255, blue: 18 / 255, alpha: 1)
            })
        static let rideAccent = Color(
            uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? UIColor(red: 1, green: 223 / 255, blue: 88 / 255, alpha: 1)
                    : UIColor(red: 1, green: 222 / 255, blue: 84 / 255, alpha: 1)
            })
        static let rideControlFill = Color(
            uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? UIColor(red: 50 / 255, green: 55 / 255, blue: 42 / 255, alpha: 1)
                    : UIColor(red: 17 / 255, green: 19 / 255, blue: 18 / 255, alpha: 1)
            })
        static let rideMuted = Color(
            uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? UIColor(red: 175 / 255, green: 181 / 255, blue: 174 / 255, alpha: 1)
                    : UIColor(red: 86 / 255, green: 91 / 255, blue: 88 / 255, alpha: 1)
            })
        static let rideLine = Color(
            uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? UIColor(red: 48 / 255, green: 54 / 255, blue: 47 / 255, alpha: 1)
                    : UIColor(red: 216 / 255, green: 219 / 255, blue: 211 / 255, alpha: 1)
            })
        static let disabledFill = pageBackground
        static let brand = Color(
            uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? UIColor(red: 1.0, green: 0.84, blue: 0.15, alpha: 1)
                    : UIColor(red: 0.48, green: 0.29, blue: 0.0, alpha: 1)
            })
        static let yellow = Color(red: 1, green: 0.8, blue: 0)
        static let cyan = Color(uiColor: .systemCyan)
        static let orange = Color(uiColor: .systemOrange)
        static let red = Color(uiColor: .systemRed)
        static let teal = Color(uiColor: .systemTeal)
        static let brown = Color(uiColor: .systemBrown)
        static let cardFill = Color(uiColor: .secondarySystemBackground)
        static let primaryText = Color(uiColor: .label)
        static let mutedText = Color(uiColor: .label)
        static let green = Color(uiColor: .systemGreen)
        static let purple = Color(uiColor: .systemPurple)
        static let nominal = Color(
            uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? .systemGreen
                    : UIColor(red: 0.0, green: 0.38, blue: 0.16, alpha: 1)
            })
    #elseif os(macOS)
        static let pageBackground = Color(nsColor: .windowBackgroundColor)
        static let rideCanvas = Color(
            nsColor: NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    ? NSColor(red: 8 / 255, green: 10 / 255, blue: 9 / 255, alpha: 1)
                    : NSColor(red: 250 / 255, green: 249 / 255, blue: 244 / 255, alpha: 1)
            })
        static let rideInk = Color(
            nsColor: NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    ? NSColor(red: 248 / 255, green: 247 / 255, blue: 239 / 255, alpha: 1)
                    : NSColor(red: 17 / 255, green: 19 / 255, blue: 18 / 255, alpha: 1)
            })
        static let rideAccent = Color(
            nsColor: NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    ? NSColor(red: 1, green: 223 / 255, blue: 88 / 255, alpha: 1)
                    : NSColor(red: 1, green: 222 / 255, blue: 84 / 255, alpha: 1)
            })
        static let rideControlFill = Color(
            nsColor: NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    ? NSColor(red: 50 / 255, green: 55 / 255, blue: 42 / 255, alpha: 1)
                    : NSColor(red: 17 / 255, green: 19 / 255, blue: 18 / 255, alpha: 1)
            })
        static let rideMuted = Color(
            nsColor: NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    ? NSColor(red: 175 / 255, green: 181 / 255, blue: 174 / 255, alpha: 1)
                    : NSColor(red: 86 / 255, green: 91 / 255, blue: 88 / 255, alpha: 1)
            })
        static let rideLine = Color(
            nsColor: NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    ? NSColor(red: 48 / 255, green: 54 / 255, blue: 47 / 255, alpha: 1)
                    : NSColor(red: 216 / 255, green: 219 / 255, blue: 211 / 255, alpha: 1)
            })
        static let disabledFill = pageBackground
        static let brand = Color(
            nsColor: NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    ? NSColor(red: 1.0, green: 0.84, blue: 0.15, alpha: 1)
                    : NSColor(red: 0.48, green: 0.29, blue: 0.0, alpha: 1)
            })
        static let yellow = Color(red: 1, green: 0.8, blue: 0)
        static let cyan = Color(nsColor: .systemCyan)
        static let orange = Color(nsColor: .systemOrange)
        static let red = Color(nsColor: .systemRed)
        static let teal = Color(nsColor: .systemTeal)
        static let brown = Color(nsColor: .systemBrown)
        static let cardFill = Color(nsColor: .underPageBackgroundColor)
        static let primaryText = Color(nsColor: .labelColor)
        static let mutedText = Color(nsColor: .labelColor)
        static let green = Color(nsColor: .systemGreen)
        static let purple = Color(nsColor: .systemPurple)
        static let nominal = Color(nsColor: .systemGreen)
    #endif
}

public enum PevDashboardColors {
    public static let pageBackground = PevSystemColors.pageBackground
    public static let rideCanvas = PevSystemColors.rideCanvas
    public static let rideInk = PevSystemColors.rideInk
    public static let rideAccent = PevSystemColors.rideAccent
    public static let rideControlFill = PevSystemColors.rideControlFill
    public static let rideMuted = PevSystemColors.rideMuted
    public static let rideLine = PevSystemColors.rideLine
    public static let disabledFill = PevSystemColors.disabledFill
    public static let brand = PevSystemColors.brand
    public static let yellow = PevSystemColors.yellow
    public static let cyan = PevSystemColors.cyan
    public static let orange = PevSystemColors.orange
    public static let red = PevSystemColors.red
    public static let teal = PevSystemColors.teal
    public static let brown = PevSystemColors.brown
    public static let warningText = orange
    public static let warningFill = orange.opacity(0.14)
    public static let warningStroke = orange.opacity(0.55)
    public static let cardFill = PevSystemColors.cardFill
    public static let cardStroke = Color.secondary.opacity(0.35)
    public static let primaryText = PevSystemColors.primaryText
    public static let mutedText = PevSystemColors.mutedText
    public static let disabledText = Color.primary.opacity(0.58)
    public static let disabledSecondaryText = Color.primary.opacity(0.48)
    public static let green = PevSystemColors.green
    public static let purple = PevSystemColors.purple
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

nonisolated func pevDashboardResolvedForegroundColor(
    _ color: Color,
    contrast: ColorSchemeContrast,
    differentiateWithoutColor: Bool,
    colorScheme: ColorScheme
) -> Color {
    guard contrast == .increased || differentiateWithoutColor else { return color }
    return colorScheme == .dark ? .white : .black
}

private struct PevDashboardAccessibleForegroundStyle: ViewModifier {
    @Environment(\.colorSchemeContrast) private var colorSchemeContrast
    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor
    @Environment(\.colorScheme) private var colorScheme

    let color: Color

    func body(content: Content) -> some View {
        content.foregroundStyle(
            pevDashboardResolvedForegroundColor(
                color,
                contrast: colorSchemeContrast,
                differentiateWithoutColor: differentiateWithoutColor,
                colorScheme: colorScheme
            )
        )
    }
}

extension View {
    func pevDashboardAccessibleForegroundStyle(_ color: Color) -> some View {
        modifier(PevDashboardAccessibleForegroundStyle(color: color))
    }
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
