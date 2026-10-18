import Accessibility
import CutoutMobile
import SwiftUI

struct VescRideScreenView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @ScaledMetric(relativeTo: .headline) private var headroomLineHeight: CGFloat = 21
    @ScaledMetric(relativeTo: .caption) private var footerHeight: CGFloat = 20
    let liveSnapshot: VescRideSnapshot?
    let phase: SessionConnectionPhase
    let now: MonotonicMilliseconds
    let captureStatusText: String?
    let connectionStatusText: String?
    var disconnect: (() -> Void)? = nil
    @State private var showsStandardReadings = false

    private var presentation: VescRideScreenPresentation {
        VescRideScreenPresentation(
            snapshot: liveSnapshot,
            phase: phase,
            now: now,
            connectionStatusText: connectionStatusText
        )
    }

    var dashboardTiles: [PevDashboardTile] {
        guard liveSnapshot != nil else {
            return VescRideScreenPresentation(
                snapshot: VescRideSnapshot(
                    title: presentation.title, vehicleKind: .unknown,
                    application: .absent, controllerState: .unknown),
                phase: phase, now: now, connectionStatusText: connectionStatusText
            ).dashboardTiles
        }
        return presentation.dashboardTiles
    }

    var body: some View {
        Group {
            if presentation.usesStandardInstruments {
                standardInstruments
            } else {
                refloatInstruments
            }
        }
        .accessibilityElement(children: .contain)
        .onChange(of: liveSnapshot?.warning) { _, warning in
            if let announcement = warning?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
        .onChange(of: liveSnapshot?.stopReason) { _, reason in
            guard liveSnapshot?.warning == .some(.none) else { return }
            if let announcement = reason?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }

    private var refloatInstruments: some View {
        PevRideDashboardShell(
            heroStyle: .vesc,
            title: presentation.title,
            subtitle: presentation.subtitle,
            statusTone: presentation.statusTone,
            captureStatusText: captureStatusText,
            speedReadout: presentation.speedReadout,
            speedCaption: localizedAppText("vesc.speed.caption")
        ) {
            if dynamicTypeSize.isAccessibilitySize {
                PevRideAccessibleReadings(
                    headroomLabel: localizedAppText("vesc.duty_headroom.label"),
                    headroom: liveSnapshot?.dutyHeadroomMetricValue ?? .unavailable,
                    headroomProgress: liveSnapshot?.dutyHeadroomProgress,
                    tiles: dashboardTiles,
                    footpadText: liveSnapshot?.footpad?.stateDisplayText
                )
            } else {
                GeometryReader { geometry in
                    VStack(spacing: 8) {
                        PevDashboardProgressBar(
                            label: localizedAppText("vesc.duty_headroom.label"),
                            metricValue: liveSnapshot?.dutyHeadroomMetricValue ?? .unavailable,
                            progress: liveSnapshot?.dutyHeadroomProgress,
                            height: 10
                        )
                        .fixedSize(horizontal: false, vertical: true)
                        PevDashboardGrid(
                            columns: [GridItem(.flexible(), spacing: 8), GridItem(.flexible(), spacing: 8)],
                            spacing: 8
                        ) {
                            ForEach(dashboardTiles) { tile in
                                PevDashboardMetricTile(
                                    label: tile.kind == .motorCurrent
                                        ? localizedAppText("vesc.metric.motor_current.compact") : tile.label,
                                    metricValue: tile.metricValue, unit: tile.unit,
                                    prominence: .ride
                                )
                                .accessibilityLabel(tile.label)
                                .frame(
                                    height: min(
                                        108,
                                        max(0, (geometry.size.height - headroomLineHeight - 17 - footerHeight - 24) / 2)
                                    ))
                            }
                        }
                        Group {
                            if let footpad = liveSnapshot?.footpad {
                                Text(footpad.stateDisplayText)
                                    .font(.caption.weight(.semibold))
                                    .lineLimit(1)
                                    .minimumScaleFactor(0.6)
                            } else {
                                Color.clear
                                    .accessibilityHidden(true)
                            }
                        }
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(minHeight: 20)
                    }
                }
            }
        }
    }

    private var standardInstruments: some View {
        PevRideInstrumentLayout {
            HStack(spacing: 8) {
                PevRideStatusHeader(
                    title: presentation.title, subtitle: presentation.subtitle,
                    statusTone: presentation.statusTone, captureStatusText: captureStatusText,
                    quietStatus: true
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
        } primary: { _ in
            GeometryReader { geometry in
                PevRideSpeedReading(
                    speed: presentation.speedReadout,
                    caption: localizedAppText("vesc.speed.caption"),
                    pointSize: min(170, geometry.size.width * 0.44, geometry.size.height * 0.70)
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        } summary: {
            standardEnergyReading
        } secondary: {
            // At accessibility sizes the scrollable Readings sheet retains every secondary
            // measurement while the fixed Ride surface keeps room for speed and energy.
            if !dynamicTypeSize.isAccessibilitySize {
                standardSecondaryReadings
            }
        }
        .sheet(isPresented: $showsStandardReadings) {
            PevRideReadingsDetail(tiles: dashboardTiles, footpadText: nil)
        }
    }

    private var standardEnergyReading: some View {
        let tile = presentation.standardEnergyTile
        return Button {
            showsStandardReadings = true
        } label: {
            PevInstrumentReading(
                label: tile.label, value: tile.metricValue, unit: tile.unit,
                pointSize: 52, identifier: "vesc.ride.energy", showsDisclosure: true,
                detail: tile.detail
            )
            .visualContent
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
            .padding(.vertical, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .overlay(alignment: .top) { Rectangle().fill(PevColors.cardStroke).frame(height: 1) }
        .overlay(alignment: .bottom) { Rectangle().fill(PevColors.cardStroke).frame(height: 1) }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(tile.label)
        .accessibilityValue(tile.metricValue.accessibilityValue(unit: tile.unit, detail: tile.detail))
        .accessibilityHint(localizedAppText("ride.readings.open"))
        .accessibilityIdentifier("ride.readings.open")
    }

    private var standardSecondaryReadings: some View {
        HStack(alignment: .top, spacing: 12) {
            ForEach(presentation.standardSecondaryTiles) { tile in
                PevInstrumentReading(
                    label: tile.label, value: tile.metricValue, unit: tile.unit,
                    pointSize: 23, identifier: "vesc.ride.\(tile.kind)", detail: tile.detail
                )
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}
