import Accessibility
import CutoutMobile
import SwiftUI

struct VescRideScreenView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.verticalSizeClass) private var verticalSizeClass

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
        presentation.dashboardTiles
    }

    private var prioritizesMetrics: Bool {
        guard dynamicTypeSize.isAccessibilitySize && verticalSizeClass == .compact else {
            return false
        }
        switch presentation.dashboardSupport {
        case .telemetryStale, .telemetryPending:
            return false
        case .dutyHeadroom, .none:
            return presentation.warningCard == nil
        }
    }

    var body: some View {
        PevRideDashboardShell(
            sectionTitle: localizedAppText("navigation.section.ride"),
            heroStyle: .vescOnewheel,
            title: presentation.title,
            subtitle: presentation.subtitle,
            statusTone: presentation.statusTone,
            captureStatusText: captureStatusText,
            speedReadout: presentation.speedReadout,
            speedCaption: localizedAppText("vesc.speed.caption"),
            allowsVerticalScroll: true,
        ) {

            if prioritizesMetrics {
                metricsGrid
            }

            switch presentation.dashboardSupport {
            case let .telemetryStale(elapsed):
                PevDashboardWarningCard(
                    title: localizedAppText("vesc.warning.telemetry_stale"),
                    detail: localizedAppText(
                        "vesc.warning.telemetry_stale_detail",
                        Int64(elapsed.rawValue)
                    ),
                    accent: PevColors.primaryText,
                    detailColor: PevColors.primaryText,
                    fill: PevColors.cardFill,
                    stroke: PevColors.primaryText,
                    cornerRadius: 24
                )
                .accessibilityIdentifier("vesc.warning.telemetry-stale")
                .padding(.top, 12)
            case let .dutyHeadroom(metricValue, progress):
                PevDashboardProgressCard(
                    label: localizedAppText("vesc.duty_headroom.label"),
                    metricValue: metricValue,
                    detail: localizedAppText("vesc.duty_headroom.detail"),
                    progress: progress
                )
                .padding(.top, 12)
            case .telemetryPending:
                PevDashboardWarningCard(
                    title: localizedAppText("vesc.subtitle.telemetry_pending"),
                    detail: localizedAppText("vesc.telemetry_pending.detail"),
                    tone: .vesc
                )
                .accessibilityIdentifier("vesc.warning.telemetry-pending")
                .padding(.top, 12)
            case .none:
                EmptyView()
            }

            if let warningCard = presentation.warningCard {
                PevDashboardWarningCard(
                    title: warningCard.title,
                    detail: warningCard.detail,
                    tone: .vesc
                )
                    .accessibilityIdentifier("vesc.warning.active")
                    .padding(.top, 10)
            }

            if let footpad = liveSnapshot?.footpad {
                PevDashboardFootpadReadout(
                    leftLabel: localizedAppText("vesc.footpad.left"),
                    leftMetricValue: footpad.adc1MetricValue,
                    rightLabel: localizedAppText("vesc.footpad.right"),
                    rightMetricValue: footpad.adc2MetricValue,
                    detail: footpad.stateDisplayText
                )
                .padding(.top, 8)
            }

            if !prioritizesMetrics {
                metricsGrid
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

    private var metricsGrid: some View {
        PevDashboardGrid(columnSpacing: 12, spacing: 12) {
            ForEach(dashboardTiles) { tile in
                PevDashboardMetricTile(tile, prominence: .compactDashboard)
            }
        }
        .padding(.top, 8)
    }
}
