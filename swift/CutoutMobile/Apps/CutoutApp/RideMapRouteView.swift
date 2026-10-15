import CutoutMobile
import MapKit
import Observation
import SwiftUI

@MainActor
@Observable
final class RideMapPresentationState {
    enum Mode: String {
        case live
        case history
        case charging
    }

    var mode: Mode = .live {
        didSet { prepareCameraForMode() }
    }
    var liveMapPosition: MapCameraPosition = .automatic
    var historyMapPosition: MapCameraPosition = .automatic
    var detailMapPosition: MapCameraPosition = .automatic
    var chargingMapPosition: MapCameraPosition = .automatic
    let charging = ChargingMapModel()
    var chargingEditor: ChargingSpotEditorModel?
    var liveIsApplyingCamera = false
    var historyIsApplyingCamera = false
    var detailIsApplyingCamera = false
    var followsLatestPoint = true
    static let sharedLocalSpan = MKCoordinateSpan(latitudeDelta: 0.000_001, longitudeDelta: 0.01)
    static var defaultLocalSpan: MKCoordinateSpan { sharedLocalSpan }
    var sharedCameraSpan = RideMapPresentationState.sharedLocalSpan
    var currentLocation: CLLocationCoordinate2D?
    var historyFittedCameraID: String?

    static func defaultLocalRegion(center: CLLocationCoordinate2D) -> MKCoordinateRegion {
        MKCoordinateRegion(center: center, span: sharedLocalSpan)
    }

    static func localRegion(
        center: CLLocationCoordinate2D,
        longitudeDelta: CLLocationDegrees
    ) -> MKCoordinateRegion? {
        guard CLLocationCoordinate2DIsValid(center), longitudeDelta.isFinite,
            longitudeDelta > 0, longitudeDelta <= 360
        else { return nil }
        return MKCoordinateRegion(
            center: center,
            span: MKCoordinateSpan(latitudeDelta: 0.000_001, longitudeDelta: longitudeDelta)
        )
    }

    func recordUserCameraSpan(_ span: MKCoordinateSpan) {
        guard span.latitudeDelta.isFinite, span.longitudeDelta.isFinite,
            span.latitudeDelta > 0, span.longitudeDelta > 0,
            span.latitudeDelta <= 180, span.longitudeDelta <= 360
        else { return }
        sharedCameraSpan = MKCoordinateSpan(
            latitudeDelta: 0.000_001,
            longitudeDelta: span.longitudeDelta
        )
    }

    func updateCurrentLocation(_ coordinate: CLLocationCoordinate2D?) {
        guard let coordinate, CLLocationCoordinate2DIsValid(coordinate) else { return }
        let isFirstLocation = currentLocation == nil
        currentLocation = coordinate
        guard isFirstLocation else { return }
        initializeLocalPositionIfNeeded(&liveMapPosition)
        initializeLocalPositionIfNeeded(&historyMapPosition)
        initializeLocalPositionIfNeeded(&chargingMapPosition)
    }

    private func prepareCameraForMode() {
        switch mode {
        case .live:
            applySharedSpan(to: &liveMapPosition)
        case .history:
            applySharedSpan(to: &historyMapPosition)
        case .charging:
            prepareChargingCamera()
        }
    }

    private func prepareChargingCamera() {
        let center =
            cameraCenter(chargingMapPosition)
            ?? currentLocation
            ?? cameraCenter(liveMapPosition)
            ?? cameraCenter(historyMapPosition)
        guard let center else {
            chargingMapPosition = .userLocation(fallback: .automatic)
            return
        }
        guard
            let region = Self.localRegion(
                center: center,
                longitudeDelta: sharedCameraSpan.longitudeDelta
            )
        else { return }
        chargingMapPosition = .region(region)
    }

    private func applySharedSpan(to position: inout MapCameraPosition) {
        guard let center = cameraCenter(position) ?? currentLocation else { return }
        guard
            let region = Self.localRegion(
                center: center,
                longitudeDelta: sharedCameraSpan.longitudeDelta
            )
        else { return }
        position = .region(region)
    }

    private func initializeLocalPositionIfNeeded(_ position: inout MapCameraPosition) {
        guard !position.positionedByUser, cameraCenter(position) == nil, let currentLocation else { return }
        position = .region(Self.defaultLocalRegion(center: currentLocation))
    }

    private func cameraCenter(_ position: MapCameraPosition) -> CLLocationCoordinate2D? {
        position.region?.center ?? position.camera?.centerCoordinate
    }
}

struct RideMapRouteView: View {
    @Bindable var model: CutoutAppModel
    @Bindable var liveRide: LiveRideModel
    @Bindable var history: RideHistoryModel
    @Bindable var presentation: RideMapPresentationState
    private let openHistory: ((String) -> Void)?
    private let closeDetail: (() -> Void)?
    private let initialHistoryID: String?
    private let detailOnly: Bool
    @Environment(\.dismiss) private var dismiss

    static func liveRouteID(for snapshot: MobileRideMapSnapshotDto?) -> String {
        snapshot?.rideID ?? "live"
    }

    private func vehicleName(for identity: String?) -> String? {
        guard let identity else { return nil }
        return history.vehicleNames[identity]
            ?? model.device.rideMapVehicleName(for: identity)
    }

    init(
        model: CutoutAppModel,
        presentation: RideMapPresentationState,
        _ openHistory: ((String) -> Void)? = nil,
        initialHistoryID: String? = nil,
        detailOnly: Bool = false,
        closeDetail: (() -> Void)? = nil
    ) {
        self._model = Bindable(wrappedValue: model)
        self._liveRide = Bindable(wrappedValue: model.liveRide)
        self._history = Bindable(wrappedValue: model.rideHistory)
        self._presentation = Bindable(wrappedValue: presentation)
        self.openHistory = openHistory
        self.closeDetail = closeDetail
        self.initialHistoryID = initialHistoryID
        self.detailOnly = detailOnly
    }

    var body: some View {
        VStack(spacing: 0) {
            if detailOnly {
                detailContent
            } else {
                HStack(spacing: 0) {
                    Picker(localizedAppText("navigation.section.map"), selection: $presentation.mode) {
                        Text(localizedAppText("ride_map.mode.live")).tag(RideMapPresentationState.Mode.live)
                        Text(localizedAppText("ride_map.mode.history")).tag(RideMapPresentationState.Mode.history)
                        Text(localizedAppText("charging.mode")).tag(RideMapPresentationState.Mode.charging)
                    }
                    .pickerStyle(.segmented)
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 10)
                // SwiftUI does not consistently forward identifiers from a
                // segmented Picker itself to the UIKit accessibility tree. Keep
                // the picker as the source of truth, but expose a stable wrapper
                // for UI tests and assistive technology.
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("ride-map.mode-picker")

                switch presentation.mode {
                case .live:
                    liveContent
                case .history:
                    historyContent
                case .charging:
                    ChargingMapContentView(
                        model: presentation.charging,
                        editor: $presentation.chargingEditor,
                        mapPosition: $presentation.chargingMapPosition,
                        cameraDidChange: { context in
                            if presentation.chargingMapPosition.positionedByUser {
                                presentation.recordUserCameraSpan(context.region.span)
                            }
                        },
                        defaultCameraSpan: presentation.sharedCameraSpan,
                        phoneLocationSample: model.mapLocationSample
                    )
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
        // Keep the screen identity on one container instead of forwarding it to
        // the segmented picker and native scroll view as separate AX elements.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(detailOnly ? "ride-map.detail-screen" : "ride-map.screen")
        .onChange(of: model.mapLocationSample, initial: true) { _, sample in
            presentation.updateCurrentLocation(
                sample.map {
                    CLLocationCoordinate2D(
                        latitude: $0.latitudeDegrees,
                        longitude: $0.longitudeDegrees
                    )
                }
            )
        }
        #if DEBUG
            .modifier(CutoutUITestSavedRideReadback(history: history))
            .onAppear {
                if CutoutUITestChargingFixture.isEnabled {
                    presentation.mode = .charging
                    presentation.chargingMapPosition = .region(CutoutUITestChargingFixture.region)
                }
            }
        #endif
        #if os(iOS)
            // Ride Detail owns its compact header so it remains attached to the map
            // when the destination is pushed from either Map entry path. Leaving the
            // NavigationStack bar visible creates a second title band and pushes the
            // detail content toward the middle of the screen.
            .toolbar(.hidden, for: .navigationBar)
        #endif
    }

    private var liveContent: some View {
        RideMapLiveContentView(
            displayPoints: liveRide.displayPoints,
            routeID: Self.liveRouteID(for: liveRide.snapshot),
            projectionVersion: liveRide.projectionVersion,
            endpointMetadata: liveRide.endpointMetadata,
            cameraRegion: liveRide.cameraRegion,
            segments: liveRide.segments,
            snapshot: liveRide.snapshot,
            isInitialSnapshotPending: liveRide.isInitialSnapshotPending,
            availability: liveRide.availability,
            storageError: liveRide.storageError,
            speed: model.rideMapSpeed,
            vehicleName: model.device.rideMapVehicleName,
            mapError: liveRide.error,
            telemetryState: liveRide.telemetryState,
            pointsTruncated: liveRide.pointsTruncated,
            segmentsOmittedByBudget: liveRide.segmentsOmittedByBudget,
            canonicalBackgroundGapCount: liveRide.backgroundGapCount,
            onVisibilityChange: { liveRide.setMapVisible($0) },
            mapPosition: $presentation.liveMapPosition,
            isApplyingCamera: $presentation.liveIsApplyingCamera,
            followsLatestPoint: $presentation.followsLatestPoint,
            cameraSpan: presentation.sharedCameraSpan,
            userLocationCenter: presentation.currentLocation,
            cameraSpanDidChange: presentation.recordUserCameraSpan,
            pause: { Task { _ = await model.pauseRideMap() } },
            resume: { Task { _ = await model.resumeRideMap() } },
            save: { Task { _ = await model.saveRideMap() } },
            stop: { Task { _ = await model.stopRideMap() } },
            start: { Task { _ = await model.startGpsOnlyRide() } },
            discard: { Task { _ = await model.discardRideMap() } }
        )
    }

    private var historyContent: some View {
        RideMapHistoryContentView(
            isRecording: liveRide.snapshot?.state == .active,
            isPaused: liveRide.snapshot?.state == .paused,
            rides: history.rides,
            searchText: $history.searchText,
            canLoadMore: history.canLoadMore,
            displayPoints: history.displayPoints,
            cameraRegion: history.cameraRegion,
            defaultCameraSpan: presentation.sharedCameraSpan,
            endpointMetadata: history.endpointMetadata,
            segments: history.segments,
            projectionVersion: history.projectionVersion,
            pointsTruncated: history.pointsTruncated,
            segmentsOmittedByBudget: history.segmentsOmittedByBudget,
            isLoading: history.isLoading,
            isRouteLoading: history.routeLoading,
            historyError: history.error,
            routeError: history.routeError,
            selectedRideID: history.selectedRideID,
            dateFilter: history.dateFilter,
            vehicleFilter: history.vehicleFilter,
            includeShortRides: history.includeShortRides,
            vehicleFilterOptions: history.vehicleIdentities,
            select: { rideID in
                presentation.mode = .history
                history.selectFromHistoryList(rideID)
                openHistory?(rideID)
            },
            load: { history.reload() },
            loadMore: { history.loadMore() },
            returnToLive: {
                presentation.mode = .live
            },
            setDateFilter: { history.setDateFilter($0) },
            setVehicleFilter: { history.setVehicleFilter($0) },
            setIncludeShortRides: { history.setIncludeShortRides($0) },
            clearFilters: { history.clearFilters() },
            currentVehicleIdentity: model.device.rideMapVehicleIdentity,
            currentVehicleName: model.device.rideMapVehicleName,
            vehicleName: vehicleName(for:),
            // Detail and list intentionally share the selected route data, but not a
            // viewport projection. A detail pan must not replace the list's display
            // projection while both destinations remain alive in the navigation stack.
            cameraDidChange: { _ in },
            cameraSpanDidChange: presentation.recordUserCameraSpan,
            previouslyFittedCameraID: presentation.historyFittedCameraID,
            cameraFitDidApply: { presentation.historyFittedCameraID = $0 },
            mapPosition: $presentation.historyMapPosition,
            isApplyingCamera: $presentation.historyIsApplyingCamera
        )
        .task {
            history.reloadPreservingSelection()
        }
    }

    @ViewBuilder
    private var detailContent: some View {
        if let initialHistoryID {
            RideMapHistoryDetailView(
                initialHistoryID: initialHistoryID,
                rides: history.rides,
                displayPoints: history.detailDisplayPoints,
                routePresence: history.detailRoutePresence,
                music: RideMapHistoryMusicDetail(
                    rideID: initialHistoryID,
                    events: history.detailMusicTimeline,
                    timelineUnavailable: history.detailMusicTimelineUnavailable,
                    state: history.detailMusicState,
                    error: history.detailMusicError,
                    forgetMusicHistory: { await model.music.forgetHistory(for: $0) }
                ),
                projectionRideID: history.detailProjectionRideID,
                cameraRegion: history.detailCameraRegion,
                endpointMetadata: history.detailEndpointMetadata,
                segments: history.detailSegments,
                projectionVersion: history.detailProjectionVersion,
                cameraFitVersion: history.detailCameraFitVersion,
                pointsTruncated: history.detailPointsTruncated,
                segmentsOmittedByBudget: history.detailSegmentsOmittedByBudget,
                canonicalBackgroundGapCount: history.detailBackgroundGapCount,
                historyError: history.error,
                routeError: history.detailRouteError,
                isLoading: history.isLoading || history.detailRouteLoading,
                selectedHistoryID: history.selectedRideID,
                ensureSelection: { history.ensureSelection(requestedRideID: $0) },
                retry: { history.reload(selecting: initialHistoryID) },
                loadRoutePreview: { history.loadRoutePreview() },
                vehicleName: vehicleName(for:),
                cameraDidChange: { region in
                    history.projectDetailViewport(RideMapCanvasView.geoBounds(for: region))
                },
                mapPosition: $presentation.detailMapPosition,
                isApplyingCamera: $presentation.detailIsApplyingCamera,
                close: closeDetail ?? { dismiss() }
            )
        } else {
            RideMapHistoryDetailUnavailableState(hasError: false, retry: {})
        }
    }
}

#if DEBUG
    /// Fixture metadata lives on the existing container; it adds no visual or AX element.
    private struct CutoutUITestSavedRideReadback: ViewModifier {
        let history: RideHistoryModel

        @ViewBuilder
        func body(content: Content) -> some View {
            if let value = CutoutUITestSavedRideFixture.accessibilityValue {
                content.accessibilityValue("\(value);\(history.uiTestHistoryReadback)")
            } else {
                content
            }
        }
    }
#endif
