import Foundation
import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct RideMapSummaryView: View {
    let snapshot: MobileRideMapSnapshotDto?
    let vehicleName: String?

    var body: some View {
        if let snapshot {
            VStack(alignment: .leading, spacing: 12) {
                metricRow(for: snapshot)

                HStack(spacing: 8) {
                    Circle()
                        .fill(snapshot.state == .recording ? PevColors.green : PevColors.yellow)
                        .frame(width: 8, height: 8)
                    Text(vehicleName ?? snapshot.associatedVehicle ?? localizedAppText("ride_map.gps_only"))
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
            value: localizedAppText("ride_map.speed_unavailable"),
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
