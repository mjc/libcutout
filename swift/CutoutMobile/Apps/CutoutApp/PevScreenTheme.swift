import CutoutMobile
import SwiftUI

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
    static let disabledText = PevDashboardColors.disabledText
    static let disabledSecondaryText = PevDashboardColors.disabledSecondaryText
    static let muted = PevDashboardColors.mutedText
    static let brand = PevDashboardColors.brand
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
