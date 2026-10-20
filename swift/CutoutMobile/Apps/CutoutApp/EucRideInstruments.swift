import CutoutMobile
import SwiftUI

/// Presentation only: values and PWM applicability come from the existing ride projection.
struct EucRideInstruments: View {
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    @State private var showsReadings = false

    private let portraitSpeedPointSize: CGFloat = 100
    private let portraitGaugeWidth: CGFloat = 180

    let title: String
    let subtitle: String
    let statusTone: PevDashboardStatusPillTone
    let captureStatusText: String?
    let speed: RideHeroReadout
    let headroom: PevSafetyBar?
    let headroomNumber: String?
    let batteryProgress: Double?
    let tiles: [PevDashboardTile]
    let gpsSpeed: PevDashboardTile
    var disconnect: (() -> Void)? = nil

    var body: some View {
        GeometryReader { geometry in
            let landscape = verticalSizeClass == .compact
                || (verticalSizeClass == nil && geometry.size.width > geometry.size.height)
            let constrainedScale = geometry.size.height < 535
                ? max(0.72, min(1, (geometry.size.height - 265) / 269)) : 1
            let speedPointSize = portraitSpeedPointSize * constrainedScale
            let gaugeWidth = min(portraitGaugeWidth, max(0, geometry.size.width - 48)) * constrainedScale
            VStack(spacing: 8) {
                header
                if landscape {
                    HStack(spacing: 24) {
                        primaryInstruments(
                            landscape: true, speedPointSize: min(132, geometry.size.width * 0.2),
                            gaugeWidth: min(220, max(0, geometry.size.width * 0.34))
                        )
                        VStack(spacing: 12) {
                            energyReadings
                            secondaryReadings
                        }
                        .frame(width: min(320, geometry.size.width * 0.4))
                    }
                } else {
                    primaryInstruments(
                        landscape: false, speedPointSize: speedPointSize, gaugeWidth: gaugeWidth
                    )
                    energyReadings
                    secondaryReadings
                }
            }
            .padding(.horizontal, 24)
            .padding(.top, landscape ? 8 : 0)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.bottom, 28)
            .frame(width: geometry.size.width, height: geometry.size.height, alignment: .top)
            .foregroundStyle(PevColors.primaryText)
        }
        .background(PevColors.pageBackground)
        .sheet(isPresented: $showsReadings) {
            PevRideReadingsDetail(tiles: tiles + [gpsSpeed], footpadText: nil)
        }
    }

    private var header: some View {
        HStack(spacing: 8) {
            PevRideStatusHeader(
                title: title, subtitle: subtitle, statusTone: statusTone,
                captureStatusText: captureStatusText, quietStatus: true
            )
            if let disconnect {
                Button(action: disconnect) {
                    Image(systemName: "xmark")
                        .font(.body.weight(.semibold))
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(localizedAppText("ride.action.disconnect"))
                .accessibilityIdentifier("dashboard.disconnect")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dashboard.top.navigation")
    }

    private func primaryInstruments(landscape: Bool, speedPointSize: CGFloat, gaugeWidth: CGFloat) -> some View {
        let gaugeHeight = gaugeWidth / 2 + 60
        let layout = landscape ? AnyLayout(HStackLayout(spacing: 24)) : AnyLayout(VStackLayout(spacing: 4))
        return layout {
            speedReading(pointSize: speedPointSize)
                .frame(maxWidth: .infinity)
                .frame(height: speedPointSize * 1.15)
            EucHeadroomGauge(headroom: headroom, percentage: headroomNumber)
                .frame(width: gaugeWidth, height: gaugeHeight)
        }
    }

    private func speedReading(pointSize: CGFloat) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(speed.displayValue ?? localizedAppText("ride.hero.value.unavailable"))
                .font(.system(size: pointSize, weight: .bold))
                .monospacedDigit()
                .tracking(-pointSize * 0.055)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
                .accessibilityLabel(localizedAppText("euc.speed.caption"))
                .accessibilityValue(speed.accessibilityValue)
                .accessibilityIdentifier("ride.hero.speed")
            Text(speed.displayUnit ?? RideUnits.speedUnit)
                .font(.title3.weight(.semibold))
                .foregroundStyle(PevColors.muted)
                .accessibilityHidden(true)
        }
    }

    private var energyReadings: some View {
        HStack(alignment: .top) {
            // Limp-home distance is not a range-at-current-pace estimate.
            EucInstrumentReading(
                label: localizedAppText("euc.ride.range"), value: .unavailable,
                unit: RideUnits.distanceUnit(forSpeedUnit: speed.displayUnit ?? RideUnits.speedUnit),
                pointSize: 52, identifier: "euc.ride.range"
            )
            Spacer(minLength: 16)
            batteryReading
        }
        .padding(.vertical, 14)
        .overlay(alignment: .top) { Rectangle().fill(PevColors.cardStroke).frame(height: 1) }
        .overlay(alignment: .bottom) { Rectangle().fill(PevColors.cardStroke).frame(height: 1) }
    }

    private var batteryReading: some View {
        let battery = tile(.batteryLevel)
        let value = battery?.metricValue ?? .unavailable
        return Button {
            showsReadings = true
        } label: {
            EucInstrumentReading(
                label: localizedAppText("ride.metric.battery"), value: value,
                unit: RideUnits.percentUnit, pointSize: 52, identifier: "euc.ride.battery",
                alignment: .trailing, showsDisclosure: true, showsProgress: true, progress: batteryProgress
            )
            .visualContent
            .frame(minWidth: 44, minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(localizedAppText("ride.metric.battery"))
        .accessibilityValue(value.accessibilityValue(unit: RideUnits.percentUnit, detail: battery?.detail ?? ""))
        .accessibilityHint(localizedAppText("ride.readings.open"))
        .accessibilityIdentifier("ride.readings.open")
    }

    private var secondaryReadings: some View {
        HStack(alignment: .top, spacing: 12) {
            secondary(.packVoltage, alignment: .leading)
            Spacer(minLength: 0)
            secondary(.power, alignment: .center)
            Spacer(minLength: 0)
            secondary(.thermal, alignment: .trailing)
        }
    }

    private func secondary(_ kind: PevDashboardTileKind, alignment: HorizontalAlignment) -> some View {
        let reading = tile(kind)
        return EucInstrumentReading(
            label: secondaryLabel(kind), value: reading?.metricValue ?? .unavailable,
            unit: reading?.unit ?? "", pointSize: 23, identifier: "euc.ride.\(kind)", alignment: alignment,
            detail: reading?.detail ?? ""
        )
    }

    private func secondaryLabel(_ kind: PevDashboardTileKind) -> String {
        switch kind {
        case .packVoltage: localizedAppText("euc.ride.voltage")
        case .power: localizedAppText("euc.ride.power")
        case .thermal: localizedAppText("euc.ride.temperature")
        default: tile(kind)?.label ?? ""
        }
    }

    private func tile(_ kind: PevDashboardTileKind) -> PevDashboardTile? {
        tiles.first { $0.kind == kind }
    }
}

struct EucInstrumentReading: View {
    let label: String
    let value: PevDashboardMetricValue
    let unit: String
    let pointSize: CGFloat
    let identifier: String
    var alignment: HorizontalAlignment = .leading
    var showsDisclosure = false
    var showsProgress = false
    var progress: Double? = nil
    var detail = ""

    var body: some View {
        visualContent
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label)
            .accessibilityValue(value.accessibilityValue(unit: unit, detail: detail))
            .accessibilityIdentifier(identifier)
    }

    var visualContent: some View {
        VStack(alignment: alignment, spacing: 5) {
            HStack(spacing: 6) {
                Text(label)
                if showsDisclosure {
                    Image(systemName: "chevron.forward")
                        .font(.caption2.weight(.semibold))
                }
            }
            .font(pointSize > 30 ? .subheadline.weight(.medium) : .caption.weight(.medium))
            .foregroundStyle(PevColors.muted)
            .lineLimit(1)
            .minimumScaleFactor(0.75)
            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text(value.displayText)
                    .font(.system(size: pointSize, weight: .semibold))
                    .monospacedDigit()
                    .tracking(-pointSize * 0.035)
                Text(unit)
                    .font(pointSize > 30 ? .title3 : .caption)
                    .foregroundStyle(PevColors.muted)
            }
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            if showsProgress {
                GeometryReader { geometry in
                    Capsule().fill(PevColors.cardStroke)
                        .overlay(alignment: .leading) {
                            if let progress {
                                Capsule().fill(PevColors.primaryText)
                                    .frame(width: geometry.size.width * max(0, min(1, progress)))
                            }
                        }
                }
                .frame(width: 93, height: 4)
                .accessibilityHidden(true)
            }
        }
    }
}

struct EucHeadroomGauge: View {
    @Environment(\.layoutDirection) private var layoutDirection
    @Environment(\.colorScheme) private var colorScheme
    let headroom: PevSafetyBar?
    var percentage: String? = nil

    var displayedProgress: Double? {
        guard let headroom, case .available = headroom.metricValue else { return nil }
        return headroom.progress.map { max(0, min(1, $0)) }
    }

    var body: some View {
        GeometryReader { geometry in
            let width = min(360, geometry.size.width, max(0, geometry.size.height - 60) * 2)
            let height = width / 2
            VStack(spacing: 12) {
                Text(localizedAppText("ride.safety.pwm_headroom"))
                    .font(.title3.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                ZStack(alignment: .bottom) {
                    EucHeadroomArc()
                        .stroke(PevColors.cardStroke, style: StrokeStyle(lineWidth: 10, lineCap: .round))
                    if let progress = displayedProgress, progress > 0 {
                        EucHeadroomArc()
                            .trim(from: 0, to: progress)
                            .stroke(accent, style: StrokeStyle(lineWidth: 10, lineCap: .round))
                            .scaleEffect(x: layoutDirection == .rightToLeft ? -1 : 1, y: 1)
                    }
                    headroomValue(pointSize: min(84, width * 0.23))
                        .padding(.bottom, 4)
                }
                .frame(width: max(0, width - 12), height: max(0, height - 6))
                HStack {
                    Text(0.formatted())
                    Spacer()
                    Text(100.formatted())
                }
                .font(.caption.weight(.semibold))
                .foregroundStyle(PevColors.muted)
                .frame(width: max(0, width - 12))
                .accessibilityHidden(true)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(localizedAppText("ride.safety.pwm_headroom"))
        .accessibilityValue(
            headroom?.metricValue.accessibilityText ?? PevDashboardMetricValue.unavailable.accessibilityText
        )
        .accessibilityIdentifier("euc.ride.headroom")
    }

    private func headroomValue(pointSize: CGFloat) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 4) {
            Text(percentage ?? headroom?.metricValue.displayText ?? PevDashboardMetricValue.unavailable.displayText)
                .font(.system(size: pointSize, weight: .bold))
            if percentage != nil {
                Text(RideUnits.percentUnit)
                    .font(.system(size: pointSize * 0.4, weight: .semibold))
            }
        }
        .monospacedDigit()
        .tracking(-2)
        .foregroundStyle(accent)
        .lineLimit(1)
        .minimumScaleFactor(0.5)
    }

    private var accent: Color { colorScheme == .dark ? PevColors.yellow : PevColors.primaryText }
}

private struct EucHeadroomArc: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.addArc(
            center: CGPoint(x: rect.midX, y: rect.maxY), radius: rect.width / 2,
            startAngle: .degrees(180), endAngle: .degrees(0), clockwise: false
        )
        return path
    }
}
