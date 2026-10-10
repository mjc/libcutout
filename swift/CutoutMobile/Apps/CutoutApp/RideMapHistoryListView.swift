import CutoutMobile
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
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.bottom, 12)
                .accessibilityAddTraits(.isHeader)

            LazyVStack(spacing: 8) {
                ForEach(rides, id: \.rideID) { ride in
                    RideMapHistoryRow(
                        rideID: ride.rideID,
                        isSelected: ride.rideID == selectedRideID,
                        title: rideTitle(for: ride),
                        vehicle: RideMapMetricFormatting.vehicleLabel(
                            identity: ride.associatedVehicle ?? ride.candidateVehicle,
                            resolve: { _ in ride.vehicleDisplayName },
                            noIdentityFallback: localizedAppText("ride_map.gps_only")
                        ),
                        distance: distanceText(for: ride.summary),
                        duration: RideMapMetricFormatting.durationText(for: ride.summary),
                        select: { select(ride.rideID) }
                    )
                }
            }
            if canLoadMore {
                Button(action: loadMore) {
                    Text(localizedAppText("ride_map.history_load_more"))
                        .frame(minHeight: 44)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("ride-map.history-load-more")
                .padding(.vertical, 8)
            }
        }
    }

    private func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        RideMapMetricFormatting.distanceText(for: summary)
    }

    static func selectionAccessibilityValue(isSelected: Bool) -> String {
        localizedAppText(isSelected ? "ride_map.history_selected" : "ride_map.history_not_selected")
    }

    private func rideTitle(for ride: MobileRideMapHistorySummaryDto) -> String {
        RideMapMetricFormatting.recordedAtText(for: ride.createdAtMilliseconds)
    }
}

struct RideMapHistoryRow: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let rideID: String
    let isSelected: Bool
    let title: String
    let vehicle: String
    let distance: String
    let duration: String
    let select: () -> Void

    var body: some View {
        Button(action: select) {
            HStack(alignment: .center, spacing: 12) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(title)
                        .font(.headline)
                        .fixedSize(horizontal: false, vertical: true)
                    Text(vehicle)
                        .font(.subheadline)
                        .foregroundStyle(PevColors.muted)
                        .fixedSize(horizontal: false, vertical: true)
                    if dynamicTypeSize.isAccessibilitySize {
                        measurements
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if !dynamicTypeSize.isAccessibilitySize {
                    measurements
                        .multilineTextAlignment(.trailing)
                }
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                    .accessibilityHidden(true)
            }
            .padding(12)
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
            .background(
                isSelected ? PevColors.cardFill : PevColors.pageBackground,
                in: RoundedRectangle(cornerRadius: 12)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(isSelected ? PevColors.brand : PevColors.cardStroke.opacity(0.5), lineWidth: 1)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(PevColors.primaryText)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityValue(RideMapHistoryListView.selectionAccessibilityValue(isSelected: isSelected))
        .accessibilityIdentifier("ride-map.history-\(rideID)")
    }

    private var measurements: some View {
        VStack(alignment: dynamicTypeSize.isAccessibilitySize ? .leading : .trailing, spacing: 6) {
            Text(distance).font(.subheadline.weight(.semibold))
            Text(duration).font(.caption).foregroundStyle(PevColors.muted)
        }
        .monospacedDigit()
        .fixedSize(horizontal: false, vertical: true)
    }
}
