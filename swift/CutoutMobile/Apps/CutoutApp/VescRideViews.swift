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
                    subProtocol: .generic, controllerState: .unknown),
                phase: phase, now: now, connectionStatusText: connectionStatusText
            ).dashboardTiles
        }
        return presentation.dashboardTiles
    }

    var body: some View {
        PevRideDashboardShell(
            heroStyle: .vescOnewheel,
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

}
