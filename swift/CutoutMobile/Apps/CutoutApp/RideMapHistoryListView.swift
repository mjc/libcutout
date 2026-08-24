import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct RideMapHistoryListView: View {
    let rides: [MobileRideMapHistorySummaryDto]
    let canLoadMore: Bool
    let selectedRideID: String?
    let select: (String) -> Void
    let loadMore: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Text(localizedAppText("ride_map.history_recent"))
                .font(.title3.weight(.bold))
                .accessibilityAddTraits(.isHeader)

            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 8) {
                    ForEach(rides, id: \.rideId) { ride in
                        Button {
                            select(ride.rideId)
                        } label: {
                            HStack(spacing: 12) {
                                Image(systemName: "point.topleft.down.curvedto.point.bottomright.up")
                                    .font(.title3)
                                    .foregroundStyle(ride.rideId == selectedRideID ? PevColors.yellow : PevColors.muted)
                                    .frame(width: 30)

                                VStack(alignment: .leading, spacing: 3) {
                                    Text(rideTitle(for: ride))
                                        .font(.headline)
                                        .lineLimit(1)
                                    Text(rideSubtitle(for: ride))
                                        .font(.subheadline)
                                        .foregroundStyle(PevColors.muted)
                                        .lineLimit(1)
                                }

                                Spacer(minLength: 8)
                                Image(systemName: "chevron.right")
                                    .font(.caption.weight(.bold))
                                    .foregroundStyle(PevColors.muted)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 10)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(
                                ride.rideId == selectedRideID
                                    ? PevColors.yellow.opacity(0.13)
                                    : PevColors.pageBackground.opacity(0.55),
                                in: .rect(cornerRadius: 16)
                            )
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(ride.rideId == selectedRideID ? .isSelected : [])
                        .accessibilityIdentifier("ride-map.history-\(ride.rideId)")
                    }
                }
            }
            .frame(minHeight: 120, maxHeight: 320)
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

    private func rideSubtitle(for ride: MobileRideMapHistorySummaryDto) -> String {
        let distance = distanceText(for: ride.summary)
        let points = ride.summary.pointCount.formatted()
        return "\(distance) · \(points) points"
    }

    private func rideTitle(for ride: MobileRideMapHistorySummaryDto) -> String {
        let date = Date(timeIntervalSince1970: Double(ride.createdAtMilliseconds) / 1_000)
            .formatted(.dateTime.month(.abbreviated).day().year().hour().minute())
        return date.isEmpty ? "Ride \(String(ride.rideId.prefix(8)))" : date
    }
}
