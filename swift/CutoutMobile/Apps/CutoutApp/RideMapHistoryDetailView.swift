import CutoutMobile
import CutoutMobileFFI
import MapKit
import SwiftUI

struct RideMapHistoryDetailView: View {
    let initialHistoryID: String?
    let rides: [MobileRideMapHistorySummaryDto]
    let displayPoints: [MobileRideMapRouteDisplayPoint]
    let musicTimeline: [MobileMusicRideEventDto]
    let musicHistoryState: MobileMusicHistoryStateDto?
    let musicError: MobileRideMapError?
    /// Rust's bounded projection supplies the camera; the default keeps older route-shell
    /// callers source-compatible until they pass the projection metadata through.
    var cameraRegion: MobileRideMapCameraRegion? = nil
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let projectionVersion: UInt64
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    let canonicalBackgroundGapCount: UInt64
    let historyError: MobileRideMapError?
    let routeError: MobileRideMapError?
    let isLoading: Bool
    let selectedHistoryID: String?
    let select: (String) -> Void
    let load: () -> Void
    let retry: () -> Void
    let loadRoutePreview: () -> Void
    let forgetMusicHistory: (String) -> Bool
    let vehicleName: (String?) -> String?
    let cameraDidChange: (MKCoordinateRegion) -> Void
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    let close: () -> Void

    private var selectedRide: MobileRideMapHistorySummaryDto? {
        guard let activeHistoryID else { return nil }
        return rides.first(where: { $0.rideID == activeHistoryID })
    }

    private var activeHistoryID: String? {
        Self.activeHistoryID(
            initialHistoryID: initialHistoryID,
            selectedHistoryID: selectedHistoryID
        )
    }

    private var selectionTaskID: String { initialHistoryID ?? "" }

    private var routeState: RideMapHistoryRouteState {
        if routeError != nil {
            return .error
        }
        if isLoading {
            return .loading
        }
        if selectedRide?.summary.pointCount == 0 {
            return .empty
        }
        return .ready
    }

    static func resolvedVehicleLabel(
        associatedVehicle: String?,
        candidateVehicle: String?,
        resolve: (String) -> String?,
        fallback: String
    ) -> String {
        let identity = associatedVehicle ?? candidateVehicle
        return identity.flatMap(resolve)
            .flatMap { $0.isEmpty ? nil : $0 }
            ?? (identity == nil ? fallback : localizedAppText("ride_map.vehicle_name_unavailable"))
    }

    static func averageSpeedText(
        millimetresPerSecond: UInt64?,
        locale: Locale = .current
    ) -> String {
        guard let millimetresPerSecond else {
            return localizedAppText("ride_map.speed_unavailable")
        }
        let metersPerSecond = Double(millimetresPerSecond) / 1_000
        guard metersPerSecond.isFinite, metersPerSecond >= 0 else {
            return localizedAppText("ride_map.speed_unavailable")
        }
        let displayUnit: UnitSpeed =
            locale.measurementSystem == .metric
            ? .kilometersPerHour
            : .milesPerHour
        return Measurement(value: metersPerSecond, unit: UnitSpeed.metersPerSecond)
            .converted(to: displayUnit)
            .formatted(Measurement<UnitSpeed>.FormatStyle(width: .abbreviated).locale(locale))
    }

    static func mapHeight(for availableHeight: CGFloat) -> CGFloat {
        guard availableHeight > 0 else { return 0 }
        return min(availableHeight, min(max(availableHeight * 0.58, 240), 520))
    }

    @MainActor
    static func shouldSelectHistory(
        initialHistoryID: String?,
        selectedHistoryID: String?,
        availableHistoryIDs: [String]
    ) -> Bool {
        guard let initialHistoryID,
            availableHistoryIDs.contains(initialHistoryID)
        else {
            return false
        }
        return selectedHistoryID != initialHistoryID
    }

    static func routeID(for historyID: String?) -> String {
        historyID ?? "history-detail"
    }

    @MainActor
    static func activeHistoryID(initialHistoryID: String?, selectedHistoryID: String?) -> String? {
        selectedHistoryID ?? initialHistoryID
    }

    var body: some View {
        VStack(spacing: 0) {
            RideMapHistoryDetailHeader(close: close)

            GeometryReader { proxy in
                ScrollView(.vertical, showsIndicators: false) {
                    VStack(spacing: 0) {
                        RideMapHistoryDetailMap(
                            points: displayPoints,
                            routeID: Self.routeID(for: activeHistoryID),
                            projectionVersion: projectionVersion,
                            endpointMetadata: endpointMetadata,
                            cameraRegion: cameraRegion,
                            segments: segments,
                            state: routeState,
                            mapPosition: $mapPosition,
                            isApplyingCamera: $isApplyingCamera,
                            cameraDidChange: cameraDidChange
                        )
                        .frame(height: Self.mapHeight(for: proxy.size.height))

                        if let ride = selectedRide {
                            RideMapHistoryDetailSummary(
                                distance: distanceText(for: ride.summary),
                                duration: durationText(for: ride.summary),
                                averageSpeed: Self.averageSpeedText(
                                    millimetresPerSecond: ride.summary
                                        .averageSpeedMillimetresPerSecond,
                                    locale: .current
                                ),
                                recordedAt: recordedAtText(for: ride.createdAtMilliseconds),
                                vehicle: vehicleLabel(for: ride),
                                telemetryState: ride.telemetryState,
                                displayPointCount: displayPoints.count,
                                recordedPointCount: ride.summary.pointCount,
                                pointsTruncated: pointsTruncated,
                                segmentCount: ride.segmentCount,
                                segments: segments,
                                segmentsOmittedByBudget: segmentsOmittedByBudget,
                                canonicalBackgroundGapCount: canonicalBackgroundGapCount,
                                musicTimeline: musicTimeline,
                                musicHistoryState: musicHistoryState,
                                musicError: musicError,
                                forgetMusicHistory: {
                                    guard let activeHistoryID else { return false }
                                    return forgetMusicHistory(activeHistoryID)
                                },
                                state: routeState,
                                loadRoutePreview: loadRoutePreview,
                                shareText: shareText(for: ride),
                                mapPosition: $mapPosition,
                                isApplyingCamera: $isApplyingCamera
                            )
                        } else if activeHistoryID != nil && routeState != .loading {
                            RideMapHistoryDetailUnavailableState(
                                hasError: historyError != nil || routeError != nil,
                                retry: retry
                            )
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .task(id: selectionTaskID, loadSelectionIfNeeded)
        .accessibilityIdentifier("ride-map.detail")
    }

    private func loadSelectionIfNeeded() {
        guard let initialHistoryID else {
            if rides.isEmpty { load() }
            return
        }
        if rides.contains(where: { $0.rideID == initialHistoryID }) {
            guard
                Self.shouldSelectHistory(
                    initialHistoryID: initialHistoryID,
                    selectedHistoryID: selectedHistoryID,
                    availableHistoryIDs: rides.map(\.rideID)
                )
            else {
                return
            }
            select(initialHistoryID)
        } else {
            load()
        }
    }

    private func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }

    private func durationText(for summary: MobileRideMapSummaryDto) -> String {
        Duration.seconds(Double(summary.durationMilliseconds) / 1_000)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
    }

    private func recordedAtText(for milliseconds: UInt64) -> String {
        guard milliseconds > 0 else {
            return localizedAppText("ride_map.untitled_ride")
        }
        return Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
            .formatted(.dateTime.month(.abbreviated).day().year().hour().minute())
    }

    private func vehicleLabel(for ride: MobileRideMapHistorySummaryDto) -> String {
        if let displayName = ride.vehicleDisplayName,
            !displayName.isEmpty
        {
            return displayName
        }
        return Self.resolvedVehicleLabel(
            associatedVehicle: ride.associatedVehicle,
            candidateVehicle: ride.candidateVehicle,
            resolve: { vehicleName($0) },
            fallback: localizedAppText("ride_map.gps_only")
        )
    }

    private func shareText(for ride: MobileRideMapHistorySummaryDto) -> String {
        let distance = distanceText(for: ride.summary)
        let duration = durationText(for: ride.summary)
        let title = localizedAppText("ride_map.detail_title")
        return
            "\(title)\n\(distance) · \(duration) · \(RideMapHistoryListView.pointCountText(ride.summary.pointCount))"
    }
}
