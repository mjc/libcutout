import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct RideMapHistoryListView: View {
    let rides: [MobileRideMapHistorySummaryDto]
    @Binding var searchText: String
    let canLoadMore: Bool
    let selectedRideID: String?
    let select: (String) -> Void
    let loadMore: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            List(rides, id: \.rideId) { ride in
                Button {
                    select(ride.rideId)
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(ride.rideId)
                            .font(.headline)
                        Text(localizedAppText("ride_map.distance", distanceText(for: ride.summary)))
                        Text(localizedAppText("ride_map.points", ride.summary.pointCount))
                        if let vehicle = ride.associatedVehicle {
                            Text(localizedAppText("ride_map.associated_vehicle", vehicle))
                        } else if let candidate = ride.candidateVehicle {
                            Text(localizedAppText("ride_map.candidate_vehicle", candidate))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .contentShape(Rectangle())
                .buttonStyle(.borderless)
                .listRowBackground(
                    ride.rideId == selectedRideID
                        ? PevColors.cardStroke.opacity(0.28)
                        : Color.clear
                )
                .accessibilityIdentifier("ride-map.history-\(ride.rideId)")
            }
            .frame(maxHeight: 240)
            .searchable(text: $searchText)
            if canLoadMore {
                Button(localizedAppText("ride_map.history_load_more"), action: loadMore)
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("ride-map.history-load-more")
                    .padding(.vertical, 8)
            }
        }
    }

    private func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }
}
