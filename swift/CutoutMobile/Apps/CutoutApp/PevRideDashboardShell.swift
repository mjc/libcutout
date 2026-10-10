import CutoutMobile
import SwiftUI

struct PevRideDashboardShell<Content: View>: View {
    @Environment(\.musicCompactPlayerFrame) private var musicCompactPlayerFrame
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let heroStyle: PevRideHeroStyle
    let title: String
    let subtitle: String
    let statusTone: PevDashboardStatusPillTone
    let captureStatusText: String?
    let speedReadout: RideHeroReadout
    let speedCaption: String
    let content: Content
    var gpsSpeed: PevDashboardTile?

    init(
        heroStyle: PevRideHeroStyle,
        title: String,
        subtitle: String,
        statusTone: PevDashboardStatusPillTone,
        captureStatusText: String?,
        speedReadout: RideHeroReadout,
        speedCaption: String,
        gpsSpeed: PevDashboardTile? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.heroStyle = heroStyle
        self.title = title
        self.subtitle = subtitle
        self.statusTone = statusTone
        self.captureStatusText = captureStatusText
        self.speedReadout = speedReadout
        self.speedCaption = speedCaption
        self.content = content()
        self.gpsSpeed = gpsSpeed
    }

    var body: some View {
        GeometryReader { geometry in
            let frame = geometry.frame(in: .global)
            let safeBottom = frame.maxY - geometry.safeAreaInsets.bottom
            let unobscuredBottom = min(safeBottom, musicCompactPlayerFrame?.minY ?? safeBottom)
            let availableHeight = max(0, unobscuredBottom - frame.minY)
            let landscape = geometry.size.width > availableHeight
            let layout = landscape ? AnyLayout(HStackLayout(spacing: 16)) : AnyLayout(VStackLayout(spacing: 8))
            layout {
                PevRideHeroSection(
                    style: heroStyle,
                    title: title,
                    subtitle: subtitle,
                    statusTone: statusTone,
                    captureStatusText: captureStatusText,
                    speedReadout: speedReadout,
                    speedCaption: speedCaption,
                    speedPointSize: min(124, max(48, availableHeight * (landscape ? 0.35 : 0.20))),
                    gpsSpeed: gpsSpeed,
                    compactLayout: landscape
                )
                .frame(maxWidth: .infinity)
                .frame(
                    height: landscape || dynamicTypeSize.isAccessibilitySize
                        ? nil : min(280, max(130, availableHeight * 0.42)))
                content
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 8)
            .frame(width: geometry.size.width, height: availableHeight, alignment: .top)
            .foregroundStyle(PevColors.primaryText)
        }
        .background(PevColors.pageBackground)
    }
}

/// Keeps essential instruments on the fixed Ride surface at accessibility sizes.
/// The detail sheet renders the same Rust-derived tiles without reducing the requested text size.
struct PevRideAccessibleReadings: View {
    let headroomLabel: String
    let headroom: PevDashboardMetricValue
    let headroomProgress: Double?
    let tiles: [PevDashboardTile]
    var footpadText: String?
    @State private var showsReadings = false

    private var primaryTile: PevDashboardTile? {
        tiles.first { $0.kind == .batteryLevel } ?? tiles.first { $0.kind == .batteryVoltage }
            ?? tiles.first { $0.kind == .packVoltage }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            PevDashboardProgressBar(
                label: headroomLabel, metricValue: headroom, progress: headroomProgress, height: 10
            )
            .fixedSize(horizontal: false, vertical: true)
            Button {
                showsReadings = true
            } label: {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    if let primaryTile {
                        PevRideAccessibleReading(tile: primaryTile)
                    } else {
                        Text(localizedAppText("ride.readings.title"))
                            .font(.callout.weight(.semibold))
                    }
                    Image(systemName: "chevron.forward")
                        .font(.caption.weight(.semibold))
                        .accessibilityHidden(true)
                }
                .frame(maxWidth: .infinity, minHeight: 44)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel(primaryTile?.label ?? localizedAppText("ride.readings.title"))
            .accessibilityValue(
                primaryTile.map {
                    $0.metricValue.accessibilityValue(unit: $0.unit, detail: $0.detail)
                } ?? ""
            )
            .accessibilityHint(localizedAppText("ride.readings.open"))
            .accessibilityIdentifier("ride.readings.open")
        }
        .sheet(isPresented: $showsReadings) {
            PevRideReadingsDetail(tiles: tiles, footpadText: footpadText)
        }
    }
}

private struct PevRideAccessibleReading: View {
    let tile: PevDashboardTile

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(tile.label)
                .font(.caption.weight(.semibold))
            Spacer(minLength: 4)
            Text(tile.value)
                .font(.headline.weight(.bold).monospacedDigit())
                .fixedSize()
            if !tile.unit.isEmpty {
                Text(tile.unit)
                    .font(.caption.weight(.semibold))
                    .fixedSize()
            }
        }
    }
}

private struct PevRideReadingsDetail: View {
    @Environment(\.dismiss) private var dismiss
    let tiles: [PevDashboardTile]
    let footpadText: String?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(tiles) { tile in
                        PevDashboardMetricTile(tile)
                    }
                    if let footpadText {
                        Text(footpadText)
                            .font(.body)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(16)
            }
            .background(PevColors.pageBackground)
            .navigationTitle(localizedAppText("ride.readings.title"))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        dismiss()
                    } label: {
                        Text(localizedAppText("ride.readings.done"))
                            .frame(minWidth: 44, minHeight: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("ride.readings.done")
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("ride.readings.detail")
    }
}
