import CutoutMobile
import CutoutMobileFFI
import MapKit
import SwiftUI

struct RideMapHistoryMusicDetail {
    let rideID: String
    let events: [MobileMusicRideEventDto]
    let timelineUnavailable: Bool
    let state: MobileMusicHistoryStateDto?
    let error: MobileRideMapError?
    private let forgetMusicHistory: @MainActor (String) async -> Bool

    init(
        rideID: String,
        events: [MobileMusicRideEventDto],
        timelineUnavailable: Bool = false,
        state: MobileMusicHistoryStateDto? = nil,
        error: MobileRideMapError? = nil,
        forgetMusicHistory: @escaping @MainActor (String) async -> Bool
    ) {
        self.rideID = rideID
        self.events = events
        self.timelineUnavailable = timelineUnavailable
        self.state = state
        self.error = error
        self.forgetMusicHistory = forgetMusicHistory
    }

    var canForget: Bool {
        !events.isEmpty || state == .redacted || state == .humanReadable
    }

    func forget() async -> Bool {
        await forgetMusicHistory(rideID)
    }
}

struct RideMapHistoryDetailView: View {
    let initialHistoryID: String?
    let rides: [MobileRideMapHistorySummaryDto]
    let displayPoints: [MobileRideMapRouteDisplayPoint]
    var routePresence: MobileRideMapRoutePresence = .emptyRide
    let music: RideMapHistoryMusicDetail
    let projectionRideID: String?
    /// Rust's bounded projection supplies the camera; the default keeps older route-shell
    /// callers source-compatible until they pass the projection metadata through.
    var cameraRegion: MobileRideMapCameraRegion? = nil
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let projectionVersion: UInt64
    var cameraFitVersion: UInt64 = 0
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    let canonicalBackgroundGapCount: UInt64
    let historyError: MobileRideMapError?
    let routeError: MobileRideMapError?
    let isLoading: Bool
    let selectedHistoryID: String?
    let ensureSelection: (String?) -> Void
    let retry: () -> Void
    let loadRoutePreview: () -> Void
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
        initialHistoryID ?? selectedHistoryID
    }

    private var selectionTaskID: String { initialHistoryID ?? "" }

    private var routeState: RideMapHistoryRouteState {
        if routeError != nil {
            return .error
        }
        if isLoading {
            return .loading
        }
        if routePresence == .emptyViewport {
            return .emptyViewport
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
        return RideMapMetricFormatting.vehicleLabel(
            identity: identity,
            resolve: resolve,
            noIdentityFallback: fallback
        )
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

    static func routeID(for historyID: String?, cameraFitVersion: UInt64 = 0) -> String {
        "\(historyID ?? "history-detail"):fit-\(cameraFitVersion)"
    }

    @MainActor
    static func activeHistoryID(initialHistoryID: String?, selectedHistoryID: String?) -> String? {
        initialHistoryID ?? selectedHistoryID
    }

    var body: some View {
        VStack(spacing: 0) {
            RideMapHistoryDetailHeader(close: close)

            GeometryReader { proxy in
                ScrollView(.vertical, showsIndicators: false) {
                    VStack(spacing: 0) {
                        if projectionRideID == activeHistoryID {
                            RideMapHistoryDetailMap(
                                points: displayPoints,
                                routeID: Self.routeID(
                                    for: activeHistoryID,
                                    cameraFitVersion: cameraFitVersion
                                ),
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
                        }

                        if projectionRideID != activeHistoryID {
                            RideMapHistoryDetailUnavailableState(
                                hasError: historyError != nil || routeError != nil,
                                retry: retry
                            )
                        } else if let ride = selectedRide {
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
                                musicTimeline: music.events,
                                musicTimelineUnavailable: music.timelineUnavailable,
                                musicHistoryState: music.state,
                                musicHistoryError: music.error,
                                musicHistoryCanForget: music.canForget,
                                forgetMusicHistory: {
                                    await music.forget()
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
        .task(id: selectionTaskID) {
            ensureSelection(initialHistoryID)
        }
        .accessibilityIdentifier("ride-map.detail")
    }

    private func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        RideMapMetricFormatting.distanceText(for: summary)
    }

    private func durationText(for summary: MobileRideMapSummaryDto) -> String {
        RideMapMetricFormatting.durationText(for: summary)
    }

    private func recordedAtText(for milliseconds: UInt64) -> String {
        RideMapMetricFormatting.recordedAtText(for: milliseconds)
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
            "\(title)\n\(distance) · \(duration) · \(RideMapMetricFormatting.pointCountText(ride.summary.pointCount))"
    }
}
