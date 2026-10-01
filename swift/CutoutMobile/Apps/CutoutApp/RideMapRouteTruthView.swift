import CutoutMobile
import SwiftUI

struct RideMapRouteTruthView: View {
    let displayedPointCount: Int
    let recordedPointCount: UInt64?
    let rustSegmentCount: UInt64
    let decision: MobileRideMapDecisionDto?
    let showsRecordedBounds: Bool
    let segmentsOmittedByBudget: Bool
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let canonicalBackgroundGapCount: UInt64
    let hasRoute: Bool?
    let telemetryState: MobileRideMapTelemetryStateDto?

    init(
        displayedPointCount: Int,
        recordedPointCount: UInt64?,
        rustSegmentCount: UInt64,
        decision: MobileRideMapDecisionDto?,
        showsRecordedBounds: Bool,
        segmentsOmittedByBudget: Bool = false,
        segments: [MobileRideMapSegmentDisplayMetadata] = [],
        canonicalBackgroundGapCount: UInt64 = 0,
        hasRoute: Bool? = nil,
        telemetryState: MobileRideMapTelemetryStateDto? = nil
    ) {
        self.displayedPointCount = displayedPointCount
        self.recordedPointCount = recordedPointCount
        self.rustSegmentCount = rustSegmentCount
        self.decision = decision
        self.showsRecordedBounds = showsRecordedBounds
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
            if segmentsOmittedByBudget {
                Label(
                    localizedAppText("ride_map.segments_omitted_by_budget"),
                    systemImage: "exclamationmark.triangle"
                )
                .font(.caption)
                .foregroundStyle(.orange)
                .accessibilityIdentifier("ride-map.segments-omitted")
            }
            if let decisionText, !decisionIsAccepted {
                Label(decisionText, systemImage: decisionSystemImage)
                    .font(.caption)
                    .foregroundStyle(decisionIsAccepted ? .green : .orange)
                    .accessibilityIdentifier("ride-map.last-decision")
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

    private var decisionText: String? {
        guard let decision else { return nil }
        switch decision {
        case .accepted:
            return localizedAppText("ride_map.decision.accepted")
        case .pending:
            return localizedAppText("ride_map.decision.pending")
        case let .rejected(reason), let .ignored(reason):
            switch reason {
            case .rideNotRecording:
                return localizedAppText("ride_map.decision.ride_not_recording")
            case .duplicateLocation:
                return localizedAppText("ride_map.decision.duplicate")
            case .timestampOutOfOrder:
                return localizedAppText("ride_map.decision.out_of_order")
            case .accuracyTooLow:
                return localizedAppText("ride_map.decision.accuracy")
            case .unrealisticJump:
                return localizedAppText("ride_map.decision.jump")
            }
        case let .storageError(message):
            return localizedAppText("ride_map.decision.storage_error", message)
        }
    }

    private var decisionIsAccepted: Bool {
        if case .accepted? = decision { return true }
        return false
    }

    private var decisionSystemImage: String {
        decisionIsAccepted ? "checkmark.circle" : "exclamationmark.triangle"
    }
}
