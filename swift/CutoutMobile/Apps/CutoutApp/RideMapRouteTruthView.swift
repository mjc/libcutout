import CutoutMobile
import SwiftUI

struct RideMapRouteTruthView: View {
    let displayedPointCount: Int
    let recordedPointCount: UInt64?
    let rustSegmentCount: UInt64
    let showsRecordedBounds: Bool
    let pointsOmittedByBudget: Bool
    let segmentsOmittedByBudget: Bool
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let canonicalBackgroundGapCount: UInt64
    let hasRoute: Bool?
    let telemetryState: MobileRideMapTelemetryStateDto?

    init(
        displayedPointCount: Int,
        recordedPointCount: UInt64?,
        rustSegmentCount: UInt64,
        showsRecordedBounds: Bool,
        pointsOmittedByBudget: Bool = false,
        segmentsOmittedByBudget: Bool = false,
        segments: [MobileRideMapSegmentDisplayMetadata] = [],
        canonicalBackgroundGapCount: UInt64 = 0,
        hasRoute: Bool? = nil,
        telemetryState: MobileRideMapTelemetryStateDto? = nil
    ) {
        self.displayedPointCount = displayedPointCount
        self.recordedPointCount = recordedPointCount
        self.rustSegmentCount = rustSegmentCount
        self.showsRecordedBounds = showsRecordedBounds
        self.pointsOmittedByBudget = pointsOmittedByBudget
        self.segmentsOmittedByBudget = segmentsOmittedByBudget
        self.segments = segments
        self.canonicalBackgroundGapCount = canonicalBackgroundGapCount
        self.hasRoute = hasRoute
        self.telemetryState = telemetryState
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if Self.shouldShowBackgroundGapCount(
                routeIsPresent: routeIsPresent,
                canonicalBackgroundGapCount: backgroundGapCount
            ) {
                Text(localizedAppText("ride_map.route_interrupted"))
                    .font(.caption)
                    .foregroundStyle(PevColors.muted)
            }
            if Self.shouldShowRoutePreview(
                routeIsPresent: routeIsPresent,
                pointsOmittedByBudget: pointsOmittedByBudget,
                segmentsOmittedByBudget: segmentsOmittedByBudget
            ) {
                Label(
                    localizedAppText("ride_map.segments_omitted_by_budget"),
                    systemImage: "exclamationmark.triangle"
                )
                .font(.caption)
                .foregroundStyle(.orange)
                .accessibilityIdentifier("ride-map.segments-omitted")
            }
        }
        .accessibilityElement(children: .combine)
    }

    static func shouldShowBackgroundGapCount(
        routeIsPresent: Bool,
        canonicalBackgroundGapCount: UInt64
    ) -> Bool {
        routeIsPresent && canonicalBackgroundGapCount > 0
    }

    static func shouldShowRoutePreview(
        routeIsPresent: Bool,
        pointsOmittedByBudget: Bool,
        segmentsOmittedByBudget: Bool
    ) -> Bool {
        routeIsPresent && (pointsOmittedByBudget || segmentsOmittedByBudget)
    }

    static func routeExists(
        recordedPointCount: UInt64?,
        displayedPointCount: Int
    ) -> Bool {
        recordedPointCount.map { $0 > 0 } ?? (displayedPointCount > 0)
    }

    private var backgroundGapCount: UInt64 {
        canonicalBackgroundGapCount
    }

    private var routeIsPresent: Bool {
        hasRoute
            ?? Self.routeExists(
                recordedPointCount: recordedPointCount,
                displayedPointCount: displayedPointCount
            )
    }
}
