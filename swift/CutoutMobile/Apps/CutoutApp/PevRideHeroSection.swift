import CutoutMobile
import SwiftUI

enum PevRideHeroStyle {
    case electricUnicycle
    case vescOnewheel

    static let electricUnicycleSpeedPointSize: CGFloat = 138
    static let vescOnewheelSpeedPointSize: CGFloat = 124
    static let unitPointSize: CGFloat = 24

    var speedPointSize: CGFloat {
        switch self {
        case .electricUnicycle: Self.electricUnicycleSpeedPointSize
        case .vescOnewheel: Self.vescOnewheelSpeedPointSize
        }
    }
}

enum PevRideHeroReadout: Equatable {
    case available(value: String, unit: String)
    case unavailable

    var displayValue: String {
        switch self {
        case .available(let value, _): value
        case .unavailable: "Unavailable"
        }
    }

    var displayUnit: String {
        switch self {
        case .available(_, let unit): unit
        case .unavailable: ""
        }
    }

    var accessibilityValue: String {
        switch self {
        case .available(let value, let unit):
            [value, unit].filter { !$0.isEmpty }.joined(separator: ", ")
        case .unavailable:
            "unavailable"
        }
    }

    var isAvailable: Bool {
        if case .available = self { true } else { false }
    }
}

struct PevRideHeroSection: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @ScaledMetric(relativeTo: .largeTitle) private var eucSpeedFontSize = PevRideHeroStyle.electricUnicycleSpeedPointSize
    @ScaledMetric(relativeTo: .largeTitle) private var vescSpeedFontSize = PevRideHeroStyle.vescOnewheelSpeedPointSize
    @ScaledMetric(relativeTo: .title2) private var speedUnitFontSize = PevRideHeroStyle.unitPointSize

    let style: PevRideHeroStyle
    let title: String
    let subtitle: String
    let statusFill: Color
    let captureStatusText: String?
    let speedReadout: PevRideHeroReadout
    let speedCaption: String

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .center, spacing: 12) {
                titleText
                Spacer(minLength: 8)
                statusPill
            }
            VStack(alignment: .leading, spacing: 8) {
                titleText
                statusPill
            }
        }
        .padding(.top, 8)

        if let captureStatusText {
            PevStatusStrip(
                text: captureStatusText,
                scale: 1,
                indicatorColor: PevColors.green,
                background: PevColors.cardFill,
                foreground: PevColors.primaryText,
                cornerRadius: 18
            )
        }

        VStack(alignment: .center, spacing: 2) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: 9) {
                    speed
                    unit
                }
                VStack(spacing: 2) {
                    speed
                    unit
                }
            }
            Text(speedCaption)
                .font(.caption.weight(.bold))
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity)
        .foregroundStyle(PevColors.primaryText)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(speedCaption)
        .accessibilityValue(speedReadout.accessibilityValue)
        .accessibilityIdentifier("ride.hero.speed")
    }

    private var titleText: some View {
        Text(title)
            .font(.headline)
            .foregroundStyle(PevColors.primaryText)
            .accessibilityHeading(.h1)
    }

    private var statusPill: some View {
        PevDashboardStatusPill(
            title: subtitle,
            scale: 1,
            fill: statusFill
        )
    }

    @ViewBuilder
    private var speed: some View {
        if speedReadout.isAvailable {
            Text(speedReadout.displayValue)
                .font(speedFont)
                .monospacedDigit()
        } else {
            Text(speedReadout.displayValue)
                .font(.title2.weight(.semibold))
        }
    }

    private var speedFontSize: CGFloat {
        switch style {
        case .electricUnicycle: eucSpeedFontSize
        case .vescOnewheel: vescSpeedFontSize
        }
    }

    private var speedFont: Font {
        dynamicTypeSize.isAccessibilitySize
            ? .largeTitle.weight(.black)
            : .system(size: speedFontSize, weight: .black)
    }

    @ViewBuilder
    private var unit: some View {
        if !speedReadout.displayUnit.isEmpty {
            Text(speedReadout.displayUnit)
                .font(unitFont)
                .foregroundStyle(PevColors.muted)
        }
    }

    private var unitFont: Font {
        dynamicTypeSize.isAccessibilitySize
            ? .title2.weight(.bold)
            : .system(size: speedUnitFontSize, weight: .bold)
    }
}
