import Foundation
import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct RideMapSummaryView: View {
    let snapshot: MobileRideMapSnapshotDto?

    var body: some View {
        if let snapshot {
            VStack(alignment: .leading, spacing: 12) {
                HStack(alignment: .firstTextBaseline, spacing: 0) {
                    RideMapMetric(
                        value: distanceText(for: snapshot),
                        label: localizedAppText("ride_map.metric_distance")
                    )
                    RideMapMetric(
                        value: durationText(for: snapshot),
                        label: localizedAppText("ride_map.metric_elapsed")
                    )
                    RideMapMetric(
                        value: "—",
                        label: localizedAppText("ride_map.metric_speed")
                    )
                }

                HStack(spacing: 8) {
                    Circle()
                        .fill(PevColors.green)
                        .frame(width: 8, height: 8)
                    Text(snapshot.associatedVehicle ?? localizedAppText("ride_map.gps_only"))
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
            Text(localizedAppText("ride_map.no_active"))
                .font(.subheadline)
                .accessibilityIdentifier("ride-map.no-active")
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
}

private struct RideMapMetric: View {
    let value: String
    let label: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(value)
                .font(.title2.weight(.bold).monospacedDigit())
                .lineLimit(1)
                .minimumScaleFactor(0.72)
            Text(label)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
