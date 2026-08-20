import CutoutMobile
import SwiftUI
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

private enum PevSystemColors {
    #if os(iOS)
    static let brand = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 1.0, green: 0.84, blue: 0.15, alpha: 1)
            : UIColor(red: 0.48, green: 0.29, blue: 0.0, alpha: 1)
    })
    #elseif os(macOS)
    static let brand = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(red: 1.0, green: 0.84, blue: 0.15, alpha: 1)
            : NSColor(red: 0.48, green: 0.29, blue: 0.0, alpha: 1)
    })
    #endif
}

extension DevicePickerRow {
    var glyphColor: Color {
        switch glyphKind {
        case .scooter:
            PevColors.teal
        case .hoverboard:
            PevColors.brown
        case .electricUnicycle, .onewheel, .systemSymbol:
            PevColors.yellow
        }
    }

    var glyphBackground: Color {
        glyphColor.opacity(isSupported ? 0.12 : 0.16)
    }

    var titleColor: Color {
        isSupported ? PevColors.primaryText : PevColors.disabledText
    }

    var secondaryTextColor: Color {
        isSupported ? PevColors.muted : PevColors.disabledSecondaryText
    }
}

enum PevColors {
    // Semantic system colors keep the visual hierarchy intact while honoring
    // the user's light/dark appearance and contrast settings.
    static let pageBackground = PevDashboardColors.pageBackground
    static let cardFill = PevDashboardColors.cardFill
    static let cardStroke = PevDashboardColors.cardStroke
    static let disabledFill = PevDashboardColors.disabledFill
    static let primaryText = PevDashboardColors.primaryText
    static let disabledText = Color.primary.opacity(0.58)
    static let disabledSecondaryText = Color.primary.opacity(0.48)
    static let muted = PevDashboardColors.mutedText
    static let brand = PevSystemColors.brand
    static let yellow = PevDashboardColors.yellow
    static let cyan = PevDashboardColors.cyan
    static let green = PevDashboardColors.green
    static let orange = PevDashboardColors.orange
    static let red = PevDashboardColors.red
    static let warningText = PevDashboardColors.warningText
    static let warningFill = PevDashboardColors.warningFill
    static let warningStroke = PevDashboardColors.warningStroke
    static let teal = PevDashboardColors.teal
    static let brown = PevDashboardColors.brown
    static let purple = PevDashboardColors.purple
    static let iconFill = PevDashboardColors.disabledFill
}

extension PevAccent {
    var color: Color {
        switch self {
        case .cyan:
            PevColors.cyan
        case .green:
            PevColors.green
        case .orange:
            PevColors.orange
        case .purple:
            PevColors.purple
        case .yellow:
            PevColors.yellow
        }
    }
}
