import Foundation
import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct RideMapSummaryView: View {
    let snapshot: MobileRideMapSnapshotDto?
    let speed: SpeedReadout
    let vehicleName: String?

    enum IndicatorState: Equatable {
        case associatedWithoutTelemetry
        case gpsOnly
        case paused
        case terminal
        case unavailable

        static func state(
            lifecycle: MobileRideMapStateDto?,
            associatedVehicle: String?
        ) -> Self {
            guard let lifecycle else { return .unavailable }
            switch lifecycle {
            case .active:
                return associatedVehicle == nil
                    ? .gpsOnly
                    : .associatedWithoutTelemetry
            case .paused:
                return .paused
            case .draft, .stopped, .saved, .discarded, .interrupted, .imported:
                return .terminal
            }
        }

        static func state(for snapshot: MobileRideMapSnapshotDto?) -> Self {
            state(
                lifecycle: snapshot?.state,
                associatedVehicle: snapshot?.associatedVehicle
            )
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
        identity == nil
            ? localizedAppText("ride_map.gps_only")
            : localizedAppText("ride_map.vehicle_name_unavailable")
    }

    var body: some View {
        if let snapshot {
            VStack(alignment: .leading, spacing: 12) {
                metricRow(for: snapshot)

                HStack(spacing: 8) {
                    Circle()
                        .fill(indicatorColor(for: Self.IndicatorState.state(for: snapshot)))
                        .frame(width: 8, height: 8)
                    Text(vehicleName ?? Self.vehicleLabel(for: snapshot.associatedVehicle))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(PevColors.muted)
                        .lineLimit(1)
                }
                .accessibilityElement(children: .combine)
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
        Measurement(value: snapshot.summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }

    private func durationText(for snapshot: MobileRideMapSnapshotDto) -> String {
        Duration.seconds(Double(snapshot.summary.durationMilliseconds) / 1_000)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
    }

    private func indicatorColor(for state: IndicatorState) -> Color {
        switch state {
        case .associatedWithoutTelemetry, .gpsOnly, .paused:
            PevColors.yellow
        case .terminal, .unavailable:
            PevColors.muted
        }
    }

    @ViewBuilder
    private func metricRow(for snapshot: MobileRideMapSnapshotDto) -> some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .firstTextBaseline, spacing: 0) {
                metrics(for: snapshot)
            }
            VStack(alignment: .leading, spacing: 8) {
                metrics(for: snapshot)
            }
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
            label: localizedAppText("ride_map.metric_elapsed")
        )
        RideMapMetric(
            value: Self.speedText(for: speed),
            label: localizedAppText("ride_map.metric_speed")
        )
    }
}

private struct RideMapMetric: View {
    let value: String
    let label: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(value)
                .font(.title2.weight(.bold).monospacedDigit())
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Text(label)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
