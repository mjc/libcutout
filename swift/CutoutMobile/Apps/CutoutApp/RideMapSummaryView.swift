import Foundation
import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct RideMapSummaryView: View {
    let snapshot: MobileRideMapSnapshotDto?

    var body: some View {
        if let snapshot {
            VStack(alignment: .leading, spacing: 4) {
                Text(localizedAppText("ride_map.distance", distanceText(for: snapshot)))
                Text(localizedAppText("ride_map.duration", durationText(for: snapshot)))
                Text(localizedAppText("ride_map.points", snapshot.summary.pointCount))
                if let vehicle = snapshot.associatedVehicle {
                    Text(localizedAppText("ride_map.associated_vehicle", vehicle))
                } else {
                    Text(localizedAppText("ride_map.gps_only"))
                }
            }
            .font(.subheadline.monospacedDigit())
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
