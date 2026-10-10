import CutoutMobile
import CutoutMobileFFI
import Foundation
import SwiftUI

struct RideMapSummaryView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let snapshot: MobileRideMapSnapshotDto?
    let speed: SpeedReadout
    let vehicleName: String?

    enum IndicatorState: Equatable {
        case associatedFresh
        case associatedWithoutTelemetry
        case associatedStale
        case gpsOnly
        case terminal
        case unavailable

        static func state(for snapshot: MobileRideMapSnapshotDto?) -> Self {
            guard let snapshot else { return .unavailable }
            switch snapshot.state {
            case .active, .paused:
                switch snapshot.telemetryState {
                case .associatedFresh: return .associatedFresh
                case .associatedNoTelemetry: return .associatedWithoutTelemetry
                case .associatedStale: return .associatedStale
                case .gpsOnly: return .gpsOnly
                case .unknown: return .unavailable
                }
            case .draft, .stopped, .saved, .discarded, .interrupted, .imported:
                return .terminal
            }
        }

        @MainActor
        var accessibilityValue: String {
            switch self {
            case .associatedFresh:
                localizedAppText("ride_map.wheel_data.receiving")
            case .associatedWithoutTelemetry:
                localizedAppText("ride_map.wheel_data.waiting")
            case .associatedStale, .unavailable:
                localizedAppText("ride_map.wheel_data.unavailable")
            case .gpsOnly, .terminal:
                ""
            }
        }
    }

    @MainActor
    static func speedText(for speed: SpeedReadout) -> String {
        guard speed.millimetersPerSecond != nil else {
            return localizedAppText("ride_map.speed_unavailable")
        }
        return "\(speed.displayValue) \(speed.displayUnit)"
    }

    @MainActor
    static func vehicleLabel(for identity: String?) -> String {
        RideMapMetricFormatting.vehicleLabel(
            identity: identity,
            resolve: { _ in nil },
            noIdentityFallback: localizedAppText("ride_map.gps_only")
        )
    }

    var body: some View {
        if let snapshot {
            VStack(alignment: .leading, spacing: 12) {
                metricRow(for: snapshot)

                HStack(spacing: 8) {
                    Circle()
                        .fill(indicatorColor(for: Self.IndicatorState.state(for: snapshot)))
                        .frame(width: 8, height: 8)
                        .accessibilityHidden(true)
                    Text(vehicleName ?? Self.vehicleLabel(for: snapshot.associatedVehicle))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(PevColors.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
                .accessibilityValue(Self.IndicatorState.state(for: snapshot).accessibilityValue)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)
            .background(PevColors.cardFill, in: .rect(cornerRadius: 24))
            .overlay {
                RoundedRectangle(cornerRadius: 24)
                    .stroke(PevColors.cardStroke, lineWidth: 1)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("ride-map.summary")
        } else {
            // The lifecycle title above owns the no-active state. Keeping the
            // summary slot empty avoids repeating the same message in the card.
            EmptyView()
        }
    }

    private func distanceText(for snapshot: MobileRideMapSnapshotDto) -> String {
        RideMapMetricFormatting.distanceText(for: snapshot.summary)
    }

    private func durationText(for snapshot: MobileRideMapSnapshotDto) -> String {
        RideMapMetricFormatting.durationText(for: snapshot.summary)
    }

    private func indicatorColor(for state: IndicatorState) -> Color {
        switch state {
        case .associatedFresh:
            PevColors.green
        case .associatedWithoutTelemetry, .associatedStale:
            PevColors.yellow
        case .gpsOnly, .terminal, .unavailable:
            PevColors.muted
        }
    }

    @ViewBuilder
    private func metricRow(for snapshot: MobileRideMapSnapshotDto) -> some View {
        if dynamicTypeSize.isAccessibilitySize {
            VStack(alignment: .leading, spacing: 12) { metrics(for: snapshot) }
        } else {
            HStack(alignment: .firstTextBaseline, spacing: 12) { metrics(for: snapshot) }
        }
    }

    @ViewBuilder
    private func metrics(for snapshot: MobileRideMapSnapshotDto) -> some View {
        RideMapMetric(
            value: distanceText(for: snapshot),
            label: localizedAppText("ride_map.metric_distance")
        )
        RideMapMetric(
            value: durationText(for: snapshot),
            label: localizedAppText("ride_map.metric_recording_time")
        )
        RideMapMetric(
            value: speed.millimetersPerSecond == nil ? speed.displayValue : Self.speedText(for: speed),
            label: localizedAppText(
                snapshot.liveSpeed?.source == .phoneGps
                    ? "euc.metric.gps_speed"
                    : "ride_map.metric_speed"
            ),
            spokenValue: Self.speedText(for: speed)
        )
    }
}

private struct RideMapMetric: View {
    let value: String
    let label: String
    var spokenValue: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(value)
                .font(.title2.weight(.bold).monospacedDigit())
                .fixedSize(horizontal: false, vertical: true)
            Text(label)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityValue(spokenValue ?? value)
    }
}
