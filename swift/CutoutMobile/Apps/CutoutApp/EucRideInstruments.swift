import CutoutMobile
import SwiftUI

/// Shared instrument composition; vehicle adapters supply meaning and supported readings.
struct PevRideInstrumentLayout<Header: View, Primary: View, Summary: View, Secondary: View>: View {
    @Environment(\.musicCompactPlayerFrame) private var musicCompactPlayerFrame
    let header: Header
    let primary: (Bool) -> Primary
    let summary: Summary
    let secondary: Secondary
    let spacing: CGFloat

    init(
        spacing: CGFloat = 12,
        @ViewBuilder header: () -> Header,
        @ViewBuilder primary: @escaping (Bool) -> Primary,
        @ViewBuilder summary: () -> Summary,
        @ViewBuilder secondary: () -> Secondary
    ) {
        self.header = header()
        self.primary = primary
        self.summary = summary()
        self.secondary = secondary()
        self.spacing = spacing
    }

    var body: some View {
        GeometryReader { geometry in
            let frame = geometry.frame(in: .global)
            let safeBottom = frame.maxY - geometry.safeAreaInsets.bottom
            let bottom = min(safeBottom, musicCompactPlayerFrame?.minY ?? safeBottom)
            let height = max(0, bottom - frame.minY)
            let landscape = geometry.size.width > height
            VStack(spacing: spacing) {
                header
                if landscape {
                    HStack(spacing: 24) {
                        primary(true)
                        VStack(spacing: 12) {
                            summary
                            secondary
                        }
                        .frame(width: min(320, geometry.size.width * 0.4))
                    }
                } else {
                    primary(false).frame(maxHeight: .infinity)
                    summary
                    secondary
                }
            }
            .padding(.horizontal, 24)
            .padding(.top, landscape ? 8 : 0)
            .padding(.bottom, 28)
            .frame(width: geometry.size.width, height: height, alignment: .top)
            .foregroundStyle(PevColors.primaryText)
        }
        .background(PevColors.pageBackground)
    }
}

/// The approved Ride face keeps speed above its main instrument in portrait,
/// and places them alongside one another when the available space is wide.
struct PevRidePrimaryInstruments<Accessory: View, Instrument: View>: View {
    let speed: RideHeroReadout
    let caption: String
    let landscape: Bool
    let speedAccessory: Accessory
    let instrument: (CGSize) -> Instrument

    init(
        speed: RideHeroReadout, caption: String, landscape: Bool,
        @ViewBuilder speedAccessory: () -> Accessory,
        @ViewBuilder instrument: @escaping (CGSize) -> Instrument
    ) {
        self.speed = speed
        self.caption = caption
        self.landscape = landscape
        self.speedAccessory = speedAccessory()
        self.instrument = instrument
    }

    var body: some View {
        GeometryReader { geometry in
            let speedHeight = geometry.size.height * (landscape ? 1 : 0.42)
            let speedWidth = geometry.size.width * (landscape ? 0.22 : 0.44)
            let instrumentSize = CGSize(
                width: geometry.size.width * (landscape ? 0.5 : 1),
                height: landscape ? geometry.size.height : max(0, geometry.size.height - speedHeight - 4)
            )
            let layout = landscape ? AnyLayout(HStackLayout(spacing: 24)) : AnyLayout(VStackLayout(spacing: 4))
            layout {
                VStack(spacing: 0) {
                    PevRideSpeedReading(
                        speed: speed, caption: caption,
                        pointSize: min(170, speedWidth, speedHeight * 0.88)
                    )
                    speedAccessory
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .frame(height: landscape ? nil : speedHeight)
                instrument(instrumentSize)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }
}

extension PevRidePrimaryInstruments where Accessory == EmptyView {
    init(
        speed: RideHeroReadout, caption: String, landscape: Bool,
        @ViewBuilder instrument: @escaping (CGSize) -> Instrument
    ) {
        self.init(
            speed: speed, caption: caption, landscape: landscape, speedAccessory: { EmptyView() },
            instrument: instrument)
    }
}

/// Presentation only: values and PWM applicability come from the existing ride projection.
struct EucRideInstruments: View {
    @State private var showsReadings = false

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
        PevRideInstrumentLayout {
            header
        } primary: { landscape in
            primaryInstruments(landscape: landscape)
        } summary: {
            energyReadings
        } secondary: {
            secondaryReadings
        }
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

    private func primaryInstruments(landscape: Bool) -> some View {
        PevRidePrimaryInstruments(
            speed: speed, caption: localizedAppText("euc.speed.caption"), landscape: landscape
        ) { _ in
            EucHeadroomGauge(headroom: headroom, percentage: headroomNumber)
        }
    }

    private var energyReadings: some View {
        HStack(alignment: .top) {
            // Limp-home distance is not a range-at-current-pace estimate.
            PevInstrumentReading(
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
            PevInstrumentReading(
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
        return PevInstrumentReading(
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

/// Shared speed typography; the caller supplies its typed vehicle readout and caption.
struct PevRideSpeedReading: View {
    let speed: RideHeroReadout
    let caption: String
    let pointSize: CGFloat

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(speed.displayValue ?? localizedAppText("ride.hero.value.unavailable"))
                .font(.system(size: pointSize, weight: .bold))
                .monospacedDigit()
                .tracking(-pointSize * 0.055)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
                .accessibilityLabel(caption)
                .accessibilityValue(speed.accessibilityValue)
                .accessibilityIdentifier("ride.hero.speed")
            Text(speed.displayUnit ?? RideUnits.speedUnit)
                .font(.title3.weight(.semibold))
                .foregroundStyle(PevColors.muted)
                .accessibilityHidden(true)
        }
    }
}

struct PevInstrumentReading: View {
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
        VStack(alignment: alignment, spacing: pointSize > 60 ? 18 : 5) {
            HStack(spacing: 6) {
                Text(label)
                if showsDisclosure {
                    Image(systemName: "chevron.forward")
                        .font(.caption2.weight(.semibold))
                }
            }
            .font(
                pointSize > 60
                    ? .title3.weight(.semibold)
                    : pointSize > 30 ? .subheadline.weight(.medium) : .caption.weight(.medium)
            )
            .foregroundStyle(pointSize > 60 ? PevColors.primaryText : PevColors.muted)
            .lineLimit(1)
            .minimumScaleFactor(0.75)
            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text(value.displayText)
                    .font(.system(size: pointSize, weight: .semibold))
                    .monospacedDigit()
                    .tracking(-pointSize * 0.035)
                if value != .unavailable {
                    Text(unit)
                        .font(
                            pointSize > 60
                                ? .system(size: pointSize * 0.4, weight: .semibold)
                                : pointSize > 30 ? .title3 : .caption
                        )
                        .foregroundStyle(PevColors.muted)
                }
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
