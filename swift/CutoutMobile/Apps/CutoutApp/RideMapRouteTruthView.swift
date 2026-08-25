import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct RideMapRouteTruthView: View {
    let points: [MobileRideMapPointDto]
    let recordedPointCount: UInt64?
    let rustSegmentCount: UInt64
    let decision: MobileRideMapDecisionDto?
    let showsRecordedBounds: Bool
    let hasRoute: Bool? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(localizedAppText("ride_map.route_truth", segmentCount, telemetryText))
                .font(.caption)
                .foregroundStyle(PevColors.muted)
                .accessibilityIdentifier("ride-map.route-truth")
            if routeIsPresent, showsRecordedBounds {
                Text(localizedAppText("ride_map.route_start_end", recordedPointCount ?? UInt64(points.count)))
                    .font(.caption)
                    .foregroundStyle(PevColors.muted)
                if segmentCount > 1 {
                    Text(localizedAppText("ride_map.route_gaps", segmentCount - 1))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                }
            }
            if let decisionText {
                Label(decisionText, systemImage: decisionSystemImage)
                    .font(.caption)
                    .foregroundStyle(decisionIsAccepted ? .green : .orange)
                    .accessibilityIdentifier("ride-map.last-decision")
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var segmentCount: UInt64 { rustSegmentCount }

    private var routeIsPresent: Bool { hasRoute ?? !points.isEmpty }

    private var telemetryText: String {
        guard let state = points.last?.telemetryState else {
            return localizedAppText("ride_map.telemetry.gps_only")
        }
        switch state {
        case .gpsOnly:
            return localizedAppText("ride_map.telemetry.gps_only")
        case .associatedNoTelemetry:
            return localizedAppText("ride_map.telemetry.no_telemetry")
        case .associatedFresh:
            return localizedAppText("ride_map.telemetry.fresh")
        case .associatedStale:
            return localizedAppText("ride_map.telemetry.stale")
        }
    }

    private var decisionText: String? {
        guard let decision else { return nil }
        switch decision {
        case .accepted:
            return nil
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
