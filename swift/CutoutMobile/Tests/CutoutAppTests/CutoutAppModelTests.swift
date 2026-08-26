import XCTest
@testable import CutoutApp
import CutoutMobile
import CutoutMobileFFI
import Observation
import Synchronization

final class CutoutAppModelTests: XCTestCase {
    private static let priorCaptureProgress = CaptureProgress(
        elapsedMilliseconds: 63_000,
        notificationCount: 42,
        fileSizeBytes: 12_288,
        queuedMessageCount: 0,
        writerError: nil
    )

    private func settle(
        _ state: MobileRideMapState,
        _ decision: MobileRideMapDecisionDto
    ) -> MobileRideMapDecisionDto {
        guard case .pending = decision else { return decision }
        for _ in 0 ..< 100 {
            if let terminal = state.pollLocationWrites().first {
                return terminal
            }
            Thread.sleep(forTimeInterval: 0.001)
        }
        XCTFail("timed out waiting for durable ride-map location outcome")
        return decision
    }

    override func setUp() {
        super.setUp()
        clear(RideSessionMarkerStore())
        clear(DevicePickerSelectionStore())
    }

    override func tearDown() {
        clear(RideSessionMarkerStore())
        clear(DevicePickerSelectionStore())
        super.tearDown()
    }

    private func clear(
        _ store: RideSessionMarkerStore,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        do {
            try store.clear()
        } catch {
            XCTFail("failed to clear ride session marker store: \(error)", file: file, line: line)
        }
    }

    private func clear(
        _ store: DevicePickerSelectionStore,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        do {
            try store.clear()
        } catch {
            XCTFail("failed to clear device picker selection store: \(error)", file: file, line: line)
        }
    }

    @MainActor
    func testAvailableBmsRouteDoesNotObserveRideTelemetry() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)
        driver.onBmsSnapshotChange?(
            BmsSnapshot(
                topology: BmsTopology(
                    layoutLabel: "20S1P",
                    seriesGroupCount: 20,
                    parallelCount: 1,
                    packCount: 1,
                    bmsCount: 1,
                    confidence: .verified
                )
            )
        )
        let route = EucPackRouteView(
            model: model,
            packScreen: .root,
            selectedGroupIndex: nil,
            navigate: { _ in }
        )
        XCTAssertFalse(observesChange({ _ = route.body }) {
            driver.onDisplayStateChange?(RideDisplayState(notificationCount: 1))
        })
    }

    @MainActor
    func testUnavailableBmsRouteObservesRideTelemetryForItsEstimate() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)
        let route = EucPackRouteView(
            model: model,
            packScreen: .root,
            selectedGroupIndex: nil,
            navigate: { _ in }
        )

        XCTAssertTrue(observesChange({ _ = route.body }) {
            driver.onDisplayStateChange?(RideDisplayState(notificationCount: 1))
        })
    }

    @MainActor
    func testRideMapStateRestoresTheRustSnapshotAndRouteBeforeSessionStart() async throws {
        let driver = SessionDriverSpy(rows: [])
        _ = try driver.rideMapStateHandle.startGpsOnly(
            atMs: 100,
            lastConnectedVehicle: "pev-restored"
        )
        _ = settle(driver.rideMapStateHandle, try driver.rideMapStateHandle.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 5
        ))
        XCTAssertEqual(
            try driver.rideMapStateHandle.observeVehicleConnection(
                platformIdentifier: "pev-restored",
                atMs: 200
            ),
            .associated
        )

        let model = CutoutAppModel(core: driver)

        XCTAssertEqual(model.rideMapSnapshot?.associatedVehicle, "pev-restored")
        XCTAssertEqual(model.rideMapSnapshot?.summary.pointCount, 1)
        for _ in 0 ..< 20 where model.rideMapPoints.isEmpty {
            await Task.yield()
        }
        XCTAssertEqual(model.rideMapPoints.count, 1)
    }

    @MainActor
    func testRideMapPointMetadataRetainsOnlyTheBoundedRustTail() {
        var points = [MobileRideMapPointDto]()
        for sequence in 0 ..< 4_097 {
            CutoutAppModel.appendBoundedRideMapPoint(
                MobileRideMapPointDto(
                    sequence: UInt64(sequence),
                    segmentId: 0,
                    latitudeDegrees: 39.7392,
                    longitudeDegrees: -104.9903,
                    wallClockUnixMs: 1_700_000_000_000 + UInt64(sequence),
                    monotonicMs: UInt64(sequence),
                    horizontalAccuracyMeters: 5,
                    telemetryState: .gpsOnly
                ),
                to: &points,
                limit: 4_096
            )
        }

        XCTAssertEqual(points.count, 4_096)
        XCTAssertEqual(points.first?.sequence, 1)
        XCTAssertEqual(points.last?.sequence, 4_096)
    }

    @MainActor
    func testLiveRideMapDecisionPublishesItsProjectionAsynchronously() async throws {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)
        _ = try driver.rideMapStateHandle.startGpsOnly(
            atMs: 100,
            lastConnectedVehicle: nil
        )
        let decision = settle(driver.rideMapStateHandle, try driver.rideMapStateHandle.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 5
        ))
        guard case let .accepted(point, _) = decision else {
            return XCTFail("expected the seeded location to be accepted, got \(decision)")
        }
        guard let snapshot = driver.rideMapStateHandle.currentSnapshot() else {
            return XCTFail("expected an active ride-map snapshot")
        }

        driver.onRideMapDecisionChange?(snapshot, .accepted(point: point, segmentStarted: false))

        for _ in 0 ..< 100 where model.rideMapLiveDisplayPoints.isEmpty {
            await Task.yield()
        }
        XCTAssertFalse(model.rideMapLiveDisplayPoints.isEmpty)
    }

    @MainActor
    func testRideMapCommandFailureRemainsVisibleAsTheTypedRustError() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)

        XCTAssertFalse(model.pauseRideMap())
        XCTAssertEqual(model.rideMapError, .NoActiveRide)
        XCTAssertEqual(model.rideMapLiveError, .NoActiveRide)
        XCTAssertNil(model.rideMapHistoryError)
        XCTAssertNil(model.rideMapHistoryRouteError)
    }

    @MainActor
    func testRideMapCoreStorageFailureRemainsVisibleAfterLocationDelivery() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)

        driver.onRideMapErrorChange?(.Storage("disk full"))

        XCTAssertEqual(model.rideMapError, .Storage("disk full"))
        XCTAssertNil(model.rideMapHistoryError)
        XCTAssertNil(model.rideMapHistoryRouteError)
    }

    @MainActor
    func testHistoryStorageFailureDoesNotLookLikeAnEmptyHistory() async {
        let driver = SessionDriverSpy(rows: [], rideMapStorageError: "database unavailable")
        let model = CutoutAppModel(core: driver)

        model.loadRideMapHistory()
        await Task.yield()

        XCTAssertEqual(model.rideMapHistoryError, .Storage("database unavailable"))
        XCTAssertNil(model.rideMapLiveError)
        XCTAssertFalse(model.rideMapHistoryLoading)
    }

    @MainActor
    func testRideMapLifecycleControlsUpdateRecordingState() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)

        XCTAssertFalse(model.isRideMapRecording)
        XCTAssertTrue(model.startGpsOnlyRide())
        XCTAssertTrue(model.isRideMapRecording)
        XCTAssertTrue(model.pauseRideMap())
        XCTAssertFalse(model.isRideMapRecording)
        XCTAssertTrue(model.isRideMapPaused)
        XCTAssertTrue(model.stopRideMap())
        XCTAssertFalse(model.isRideMapRecording)
        XCTAssertFalse(model.isRideMapPaused)
    }

    @MainActor
    func testHistoryReloadPreservesTheSelectedRideWhenItStillMatches() {
        XCTAssertEqual(
            CutoutAppModel.preferredHistorySelection(
                requestedID: nil,
                currentID: "ride-2",
                summaryIDs: ["ride-1", "ride-2"]
            ),
            "ride-2"
        )
        XCTAssertEqual(
            CutoutAppModel.preferredHistorySelection(
                requestedID: "ride-3",
                currentID: "ride-2",
                summaryIDs: ["ride-1", "ride-3"]
            ),
            "ride-3"
        )
        XCTAssertEqual(
            CutoutAppModel.preferredHistorySelection(
                requestedID: nil,
                currentID: "ride-missing",
                summaryIDs: ["ride-1", "ride-2"]
            ),
            "ride-1"
        )
        XCTAssertNil(
            CutoutAppModel.preferredHistorySelection(
                requestedID: "ride-missing",
                currentID: "ride-2",
                summaryIDs: ["ride-1", "ride-2"]
            )
        )
    }

    @MainActor
    func testHistoryPageAppendDoesNotDuplicateExistingRides() {
        XCTAssertEqual(
            CutoutAppModel.appendingUniqueHistory(
                existing: ["ride-1", "ride-2"],
                incoming: ["ride-2", "ride-3", "ride-1", "ride-4"],
                id: { $0 }
            ),
            ["ride-1", "ride-2", "ride-3", "ride-4"]
        )
    }

    @MainActor
    func testDetailViewportProjectionDoesNotReplaceHistoryProjection() async throws {
        let driver = SessionDriverSpy(rows: [])
        let state = driver.rideMapStateHandle
        _ = try state.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        _ = settle(state, try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7000,
            longitudeDegrees: -104.9000,
            horizontalAccuracyMeters: 5
        ))
        _ = settle(state, try state.ingestLocation(
            monotonicMs: 1_100,
            wallClockUnixMs: 1_700_000_001_100,
            latitudeDegrees: 39.7001,
            longitudeDegrees: -104.9000,
            horizontalAccuracyMeters: 5
        ))
        _ = try state.stop(atMs: 1_100)
        let rideID = try state.save().rideId

        let model = CutoutAppModel(core: driver)
        model.setRideMapHistoryDateFilter(.allTime)
        model.loadRideMapHistory(selecting: rideID)
        for _ in 0 ..< 200
        where model.selectedRideMapHistoryID != rideID || model.rideMapHistoryDisplayPoints.isEmpty
        {
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTAssertEqual(model.selectedRideMapHistoryID, rideID)
        let historyPoints = model.rideMapHistoryDisplayPoints
        XCTAssertEqual(historyPoints.count, 2)

        model.projectRideMapHistoryDetailViewport(MobileGeoBoundsDto(
            minimumLatitudeDegrees: 39.70009,
            maximumLatitudeDegrees: 39.70011,
            minimumLongitudeDegrees: -104.90001,
            maximumLongitudeDegrees: -104.89999
        ))
        for _ in 0 ..< 100 where model.rideMapHistoryDetailDisplayPoints.count != 1 {
            await Task.yield()
        }

        XCTAssertEqual(model.rideMapHistoryDetailDisplayPoints.count, 1)
        XCTAssertEqual(model.rideMapHistoryDisplayPoints, historyPoints)
    }

    @MainActor
    func testHistoryTelemetryStateUsesRustSummaryMetadata() {
        XCTAssertEqual(
            MobileRideMapHistorySummaryDto.telemetryState(
                associatedVehicle: nil,
                lastTelemetryAtMilliseconds: nil
            ),
            .gpsOnly
        )
        XCTAssertEqual(
            MobileRideMapHistorySummaryDto.telemetryState(
                associatedVehicle: "vehicle-1",
                lastTelemetryAtMilliseconds: nil
            ),
            .associatedNoTelemetry
        )
        XCTAssertEqual(
            MobileRideMapHistorySummaryDto.telemetryState(
                associatedVehicle: "vehicle-1",
                lastTelemetryAtMilliseconds: 42
            ),
            .associatedFresh
        )
    }

    func testRouteProjectionUsesRustBoundedProjection() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        for sequence in 0 ..< 10 {
            _ = settle(state, try state.ingestLocation(
                monotonicMs: UInt64(1_000 + sequence * 1_000),
                wallClockUnixMs: UInt64(1_700_000_000_000 + sequence * 1_000),
                latitudeDegrees: 39.7 + Double(sequence) / 10_000,
                longitudeDegrees: -104.9 - Double(sequence) / 10_000,
                horizontalAccuracyMeters: 5
            ))
        }

        let projection = try state.projectPoints(budget: 4)
        XCTAssertEqual(projection.sourcePointCount, 10)
        XCTAssertEqual(projection.points.count, 4)
        XCTAssertEqual(projection.points.first?.sequence, 0)
        XCTAssertEqual(projection.points.last?.sequence, 9)
    }

    func testRouteDisplayPointConversionPreservesCanonicalGeometry() {
        let point = MobileRideMapPointDto(
            sequence: 4,
            segmentId: 2,
            latitudeDegrees: 40,
            longitudeDegrees: -105,
            wallClockUnixMs: 1_700_000_000_000,
            monotonicMs: 1_000,
            horizontalAccuracyMeters: 3,
            telemetryState: .associatedFresh
        )
        let display = MobileRideMapRouteDisplayPoint(point)
        XCTAssertEqual(display.sequence, point.sequence)
        XCTAssertEqual(display.segmentId, point.segmentId)
        XCTAssertEqual(display.latitudeDegrees, point.latitudeDegrees)
        XCTAssertEqual(display.longitudeDegrees, point.longitudeDegrees)
        XCTAssertEqual(display.privacyClass, .precise)
    }

    @MainActor
    func testPickerAndCaptureRoutesDoNotObserveRideTelemetry() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)
        let picker = DevicePickerRouteView(model: model, pair: { _ in }, navigate: { _ in })
        let capture = CaptureRouteView(model: model, finishCapture: {})

        XCTAssertFalse(observesChange({ _ = picker.body }) {
            driver.onDisplayStateChange?(RideDisplayState(notificationCount: 1))
        })
        XCTAssertFalse(observesChange({ _ = capture.body }) {
            driver.onDisplayStateChange?(RideDisplayState(notificationCount: 2))
        })
    }

    @MainActor
    func testCaptureProgressInvalidatesOnlyTheCaptureRoute() {
        func observesProgressChange(
            withAvailableBms: Bool = false,
            _ render: (CutoutAppModel) -> Void
        ) -> Bool {
            let driver = SessionDriverSpy(rows: [])
            let model = CutoutAppModel(core: driver)
            if withAvailableBms {
                driver.onBmsSnapshotChange?(BmsSnapshot(
                    topology: BmsTopology(
                        layoutLabel: "20S1P",
                        seriesGroupCount: 20,
                        parallelCount: 1,
                        packCount: 1,
                        bmsCount: 1,
                        confidence: .verified
                    )
                ))
            }
            let fileURL = URL(fileURLWithPath: "/tmp/ride.cutout")
            model.applyCaptureEvent(.started(fileURL: fileURL))
            model.applyCaptureEvent(.progress(Self.priorCaptureProgress))

            return observesChange({ render(model) }) {
                model.applyCaptureEvent(.progress(CaptureProgress(
                    elapsedMilliseconds: 64_000,
                    notificationCount: 42,
                    fileSizeBytes: 16_384,
                    queuedMessageCount: 1,
                    writerError: nil
                )))
            }
        }

        XCTAssertTrue(observesProgressChange {
            _ = CaptureRouteView(model: $0, finishCapture: {}).body
        })
        XCTAssertFalse(observesProgressChange {
            _ = DevicePickerRouteView(model: $0, pair: { _ in }, navigate: { _ in }).body
        })
        XCTAssertFalse(observesProgressChange {
            _ = EucPackRouteView(model: $0, packScreen: .root, selectedGroupIndex: nil, navigate: { _ in }).body
        })
        XCTAssertFalse(observesProgressChange(withAvailableBms: true) {
            _ = EucPackRouteView(model: $0, packScreen: .root, selectedGroupIndex: nil, navigate: { _ in }).body
        })
    }

    func testCaptureQuickLabelProvidesOneStatefulActionName() {
        XCTAssertEqual(CaptureQuickLabel.ride.actionTitle(isActive: false), "Start Ride")
        XCTAssertEqual(CaptureQuickLabel.ride.actionTitle(isActive: true), "Stop Ride")
    }

    func testCaptureActionToneFollowsTheStatefulAction() {
        XCTAssertEqual(CaptureActionButtonTone.forState(isActive: false), .start)
        XCTAssertEqual(CaptureActionButtonTone.forState(isActive: true), .stop)
        XCTAssertEqual(CaptureActionButtonTone.finish, .finish)
    }

    func testRecordOnlyCaptureToneRequiresADeviceKind() {
        XCTAssertEqual(CaptureRecordActionTone.forDeviceKind(""), .requiresDeviceKind)
        XCTAssertFalse(CaptureRecordActionTone.requiresDeviceKind.isEnabled)
        XCTAssertEqual(CaptureRecordActionTone.forDeviceKind("VESC"), .ready)
        XCTAssertTrue(CaptureRecordActionTone.ready.isEnabled)
    }

    func testCaptureQuickLabelsResolveCatalogTitlesForVisibleAndAccessibleActions() {
        for label in CaptureQuickLabel.allCases {
            XCTAssertFalse(label.title.hasPrefix("capture.label."))
            XCTAssertFalse(label.actionTitle(isActive: false).hasPrefix("capture.label."))
            XCTAssertFalse(label.actionTitle(isActive: true).hasPrefix("capture.label."))
        }
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.title, "Low beam on")
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.actionTitle(isActive: false), "Start Low beam on")
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.actionTitle(isActive: true), "Stop Low beam on")
    }

    @MainActor
    func testPairFailureIsVisibleInsteadOfDoingNothing() {
        let model = CutoutAppModel()

        XCTAssertFalse(model.pair(platformIdentifier: "missing-device"))
        XCTAssertEqual(model.phase, .scanning)
        XCTAssertEqual(model.devicePickerScanState?.statusText, "Device is no longer available")
    }

    @MainActor
    func testEucRidePresentationStateOwnsInitialStateVisibility() {
        let row = DevicePickerRow(
            id: "euc-1234",
            title: "EUC",
            subtitle: "Electric unicycle",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .electricUnicycle
        )
        let model = CutoutAppModel(core: SessionDriverSpy(rows: [row]))

        XCTAssertNil(model.eucRidePresentationState)

        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: row.id))

        XCTAssertEqual(model.eucRidePresentationState?.phase, .discoveringServices)
    }

    @MainActor
    func testSupportedPickerActionStartsTheSelectedConnection() {
        let store = DevicePickerSelectionStore()
        clear(store)
        defer { clear(store) }
        let driver = SessionDriverSpy(
            rows: [
                DevicePickerRow(
                    id: "vesc-1234",
                    title: "VESC",
                    subtitle: "VESC Onewheel",
                    detail: "Device 1234",
                    state: DevicePickerRowState(action: .use),
                    symbolName: "circle.hexagongrid.circle",
                    connectionRoute: .vescOnewheel
                ),
            ]
        )
        let model = CutoutAppModel(core: driver)
        model.start()

        XCTAssertTrue(model.pair(platformIdentifier: "vesc-1234"))
        XCTAssertEqual(driver.pairedPlatformIdentifiers, ["vesc-1234"])
        XCTAssertEqual(model.phase, .discoveringServices)
        XCTAssertEqual(model.selectedRideTitle, "VESC")
        XCTAssertEqual(model.selectedConnectionRoute, .vescOnewheel)
    }

    @MainActor
    func testRepeatedUseCannotReplaceAnInFlightConnection() {
        let store = DevicePickerSelectionStore()
        clear(store)
        defer { clear(store) }
        let row = DevicePickerRow(
            id: "vesc-1234",
            title: "VESC",
            subtitle: "VESC Onewheel",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .vescOnewheel
        )
        let driver = SessionDriverSpy(rows: [row])
        let model = CutoutAppModel(core: driver)
        model.start()

        XCTAssertTrue(model.pair(platformIdentifier: row.id))
        XCTAssertFalse(model.pair(platformIdentifier: row.id))
        XCTAssertEqual(driver.pairedPlatformIdentifiers, [row.id])
    }

    @MainActor
    func testSessionStartIsIdempotentAcrossSwiftUISceneTaskRecreation() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)

        model.start()
        model.start()

        XCTAssertEqual(driver.startCount, 1)
    }

    @MainActor
    func testDelayedScanningPhaseCannotOverwriteAnActiveConnectionAttempt() {
        let suiteName = #function
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let driver = SessionDriverSpy(
            rows: [
                DevicePickerRow(
                    id: "vesc-1234",
                    title: "VESC",
                    subtitle: "VESC Onewheel",
                    detail: "Device 1234",
                    state: DevicePickerRowState(action: .use),
                    symbolName: "circle.hexagongrid.circle",
                    connectionRoute: .vescOnewheel
                ),
            ]
        )
        let model = CutoutAppModel(
            core: driver,
            selectedDeviceStore: DevicePickerSelectionStore(defaults: defaults)
        )
        model.start()

        XCTAssertTrue(model.pair(platformIdentifier: "vesc-1234"))
        driver.onPhaseChange?(.scanning)

        XCTAssertEqual(model.phase, .discoveringServices)
        XCTAssertEqual(model.selectedConnectionRoute, .vescOnewheel)
        XCTAssertEqual(driver.pairedPlatformIdentifiers, ["vesc-1234"])
    }

    @MainActor
    func testDelayedDiscoveryPhasesCannotDemoteAConnectedRide() {
        let driver = SessionDriverSpy(
            rows: [
                DevicePickerRow(
                    id: "vesc-1234",
                    title: "VESC",
                    subtitle: "VESC Onewheel",
                    detail: "Device 1234",
                    state: DevicePickerRowState(action: .use),
                    symbolName: "circle.hexagongrid.circle",
                    connectionRoute: .vescOnewheel
                ),
            ]
        )
        let model = CutoutAppModel(core: driver)
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: "vesc-1234"))
        driver.onPhaseChange?(.subscribing)
        driver.onPhaseChange?(.live)

        driver.onPhaseChange?(.starting)
        driver.onPhaseChange?(.scanning)

        XCTAssertEqual(model.phase, .live)
        XCTAssertEqual(model.connectionState.navigationIntent(isRecordOnlyCapture: false), .openRide(.vescOnewheel))
        XCTAssertEqual(driver.pairedPlatformIdentifiers, ["vesc-1234"])
    }

    @MainActor
    func testStoredDeviceAutoPairingUsesTheCandidateRideRoute() {
        let cases: [(id: String, route: DevicePickerConnectionRoute)] = [
            ("euc-1234", .electricUnicycle),
            ("vesc-1234", .vescOnewheel),
        ]

        for testCase in cases {
            let suiteName = "CutoutAppModelTests.autoPair.\(testCase.id)"
            let defaults = UserDefaults(suiteName: suiteName)!
            defaults.removePersistentDomain(forName: suiteName)
            defer { defaults.removePersistentDomain(forName: suiteName) }

            let store = DevicePickerSelectionStore(defaults: defaults)
            store.save(platformIdentifier: testCase.id)
            let row = DevicePickerRow(
                id: testCase.id,
                title: testCase.route == .vescOnewheel ? "VESC" : "EUC",
                subtitle: "Supported device",
                detail: "Device 1234",
                state: DevicePickerRowState(action: .use),
                symbolName: "circle.hexagongrid.circle",
                connectionRoute: testCase.route
            )
            let driver = SessionDriverSpy(
                rows: [row],
                notifyBluetoothRestorationOnStart: false
            )
            let markerStore = RideSessionMarkerStore(defaults: defaults)
            let model = CutoutAppModel(
                core: driver,
                selectedDeviceStore: store,
                rideSessionMarkerStore: markerStore
            )

            model.start()
            XCTAssertEqual(driver.pairedPlatformIdentifiers, [])
            driver.onBluetoothRestorationResolved?(nil)

            XCTAssertEqual(driver.pairedPlatformIdentifiers, [testCase.id])
            XCTAssertEqual(model.selectedConnectionRoute, testCase.route)
        }
    }

    @MainActor
    func testRejectedPickerActionReturnsToThePickerWithAnError() {
        let suiteName = "CutoutAppModelTests.rejectedPickerAction"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let driver = SessionDriverSpy(
            rows: [
                DevicePickerRow(
                    id: "vesc-1234",
                    title: "VESC",
                    subtitle: "VESC Onewheel",
                    detail: "Device 1234",
                    state: DevicePickerRowState(action: .use),
                    symbolName: "circle.hexagongrid.circle",
                    connectionRoute: .vescOnewheel
                ),
            ],
            pairingSucceeds: false
        )
        let model = CutoutAppModel(
            core: driver,
            selectedDeviceStore: DevicePickerSelectionStore(defaults: defaults)
        )
        model.start()

        XCTAssertFalse(model.pair(platformIdentifier: "vesc-1234"))
        XCTAssertEqual(driver.pairedPlatformIdentifiers, ["vesc-1234"])
        XCTAssertEqual(model.phase, .scanning)
        XCTAssertEqual(model.connectionState, .picker)
        XCTAssertEqual(model.devicePickerScanState?.statusText, "Device is no longer available")
        let presentation = DevicePickerConnectionPresentation(
            scanState: model.devicePickerScanState,
            phase: model.phase
        )
        XCTAssertEqual(presentation.title, "Device is no longer available")
        XCTAssertFalse(presentation.showsActivity)
    }

    @MainActor
    func testNonSupportedPickerRowCannotStartTheConnection() {
        let driver = SessionDriverSpy(
            rows: [
                DevicePickerRow(
                    id: "probe-1234",
                    title: "Probe first",
                    subtitle: "Unknown device",
                    detail: "Device 1234",
                    state: DevicePickerRowState(action: .probe),
                    symbolName: "questionmark.circle"
                ),
            ]
        )
        let model = CutoutAppModel(core: driver)
        model.start()

        XCTAssertFalse(model.pair(platformIdentifier: "probe-1234"))
        XCTAssertTrue(driver.pairedPlatformIdentifiers.isEmpty)
    }

    @MainActor
    func testProbePickerActionUsesTheProbeOperationInsteadOfRecordOnly() {
        let row = DevicePickerRow(
            id: "probe-1234",
            title: "Probe first",
            subtitle: "Unknown electric unicycle",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .probe),
            symbolName: "questionmark.circle"
        )
        let driver = SessionDriverSpy(rows: [row])
        let model = CutoutAppModel(core: driver)
        model.start()

        XCTAssertTrue(model.startProbe(platformIdentifier: row.id))
        XCTAssertEqual(driver.probedPlatformIdentifiers, [row.id])
        XCTAssertTrue(driver.recordedPlatformIdentifiers.isEmpty)
    }

    @MainActor
    func testManualPickerRowKeepsItsTypedStatusInOneAccessibleElement() {
        let row = DevicePickerRow(
            id: "manual-1234",
            title: "Add device later",
            subtitle: "Manual entry",
            detail: "Device 1234",
            state: .manual(action: "Later"),
            symbolName: "plus.circle"
        )
        let view = ManualPickerRow(row: row)

        XCTAssertEqual(view.accessibilityLabelText, row.title)
        XCTAssertEqual(view.accessibilityValueText, row.state.actionTitle)
    }

    @MainActor
    func testDisconnectKeepsSavedDeviceUntilExplicitForget() {
        let store = DevicePickerSelectionStore()
        store.save(platformIdentifier: "saved-device")
        defer { clear(store) }
        let model = CutoutAppModel()

        model.disconnectTransport()

        XCTAssertEqual(store.platformIdentifier, "saved-device")

        model.forgetSavedDevice()

        XCTAssertNil(store.platformIdentifier)
    }

    @MainActor
    func testConnectionFailureKeepsTheRememberedDeviceAndIgnoresLateConnectionCallbacks() {
        let row = DevicePickerRow(
            id: "vesc-1234",
            title: "VESC",
            subtitle: "VESC Onewheel",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .vescOnewheel
        )
        let store = DevicePickerSelectionStore()
        clear(store)
        defer { clear(store) }
        let driver = SessionDriverSpy(rows: [row])
        let model = CutoutAppModel(core: driver)
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: row.id))

        driver.onPhaseChange?(.failed(.connectFailed("timed out")))

        XCTAssertEqual(store.platformIdentifier, row.id)
        XCTAssertTrue(model.hasSavedDevice)
        XCTAssertEqual(model.phase, .failed(.connectFailed("timed out")))
        XCTAssertEqual(
            model.connectionState,
            .failed(
                ConnectionSelection(
                    platformIdentifier: row.id,
                    title: row.title,
                    route: .vescOnewheel
                ),
                .connectFailed("timed out")
            )
        )

        driver.onReconnectScheduled?(
            SessionConnectionRetry(
                platformIdentifier: row.id,
                attempt: 2,
                deadline: MonotonicMilliseconds(800),
                failure: .connectFailed("timed out")
            )
        )

        XCTAssertEqual(
            model.connectionState,
            .failed(
                ConnectionSelection(
                    platformIdentifier: row.id,
                    title: row.title,
                    route: .vescOnewheel
                ),
                .connectFailed("timed out")
            )
        )

        driver.onPhaseChange?(.discoveringServices)
        driver.onPhaseChange?(.live)

        XCTAssertEqual(model.phase, .failed(.connectFailed("timed out")))
        XCTAssertEqual(
            model.connectionState,
            .failed(
                ConnectionSelection(
                    platformIdentifier: row.id,
                    title: row.title,
                    route: .vescOnewheel
                ),
                .connectFailed("timed out")
            )
        )
    }

    @MainActor
    func testGenericConnectionProgressDoesNotInventARetryAttempt() {
        let row = DevicePickerRow(
            id: "vesc-1234",
            title: "VESC",
            subtitle: "VESC Onewheel",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .vescOnewheel
        )
        let driver = SessionDriverSpy(rows: [row])
        let model = CutoutAppModel(core: driver)
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: row.id))
        driver.onPhaseChange?(.live)

        XCTAssertEqual(
            model.connectionState,
            .connecting(
                ConnectionSelection(
                    platformIdentifier: row.id,
                    title: row.title,
                    route: .vescOnewheel
                ),
                phase: .discoveringServices
            )
        )
        XCTAssertEqual(model.phase, .discoveringServices)

        driver.onPhaseChange?(.discoveringServices)

        XCTAssertEqual(
            model.connectionState,
            .connecting(
                ConnectionSelection(
                    platformIdentifier: row.id,
                    title: row.title,
                    route: .vescOnewheel
                ),
                phase: .discoveringServices
            )
        )
        XCTAssertEqual(model.connectionStatusText, SessionConnectionPhase.discoveringServices.displayText)
        XCTAssertEqual(model.connectionState.navigationIntent(isRecordOnlyCapture: false), .stay)

        let retry = SessionConnectionRetry(
            platformIdentifier: row.id,
            attempt: 2,
            deadline: MonotonicMilliseconds(800),
            failure: .connectFailed("timed out")
        )
        driver.onReconnectScheduled?(retry)

        XCTAssertEqual(
            model.connectionState,
            .retrying(
                ConnectionSelection(
                    platformIdentifier: row.id,
                    title: row.title,
                    route: .vescOnewheel
                ),
                retry: retry
            )
        )
        XCTAssertEqual(model.connectionStatusText, "Retrying connection…")

        driver.onPhaseChange?(.live)

        XCTAssertEqual(
            model.connectionState,
            .retrying(
                ConnectionSelection(
                    platformIdentifier: row.id,
                    title: row.title,
                    route: .vescOnewheel
                ),
                retry: retry
            )
        )
        XCTAssertEqual(model.phase, .discoveringServices)

        driver.onPhaseChange?(.subscribing)
        driver.onPhaseChange?(.live)
        XCTAssertEqual(model.connectionState.navigationIntent(isRecordOnlyCapture: false), .openRide(.vescOnewheel))

        driver.onReconnectScheduled?(retry)

        XCTAssertEqual(
            model.connectionState,
            .retrying(
                ConnectionSelection(
                    platformIdentifier: row.id,
                    title: row.title,
                    route: .vescOnewheel
                ),
                retry: retry
            )
        )
        XCTAssertEqual(model.connectionStatusText, "Retrying connection…")

        driver.onPhaseChange?(.failed(.connectFailed("timed out")))
        XCTAssertEqual(model.connectionState.navigationIntent(isRecordOnlyCapture: false), .returnToPicker)
    }

    @MainActor
    func testBluetoothLossCannotLeaveRideConnected() {
        let row = DevicePickerRow(
            id: "vesc-1234",
            title: "VESC",
            subtitle: "VESC Onewheel",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .vescOnewheel
        )
        for phase in [
            SessionConnectionPhase.bluetoothPermissionDenied,
            .bluetoothUnavailable(rawState: 4),
        ] {
            let driver = SessionDriverSpy(rows: [row])
            let defaults = UserDefaults(suiteName: "CutoutAppModelTests.bluetoothLoss")!
            defaults.removePersistentDomain(forName: "CutoutAppModelTests.bluetoothLoss")
            let model = CutoutAppModel(
                core: driver,
                selectedDeviceStore: DevicePickerSelectionStore(defaults: defaults)
            )
            model.start()
            XCTAssertTrue(model.pair(platformIdentifier: row.id))
            driver.onPhaseChange?(.subscribing)
            driver.onPhaseChange?(.live)
            XCTAssertEqual(
                model.connectionState.navigationIntent(isRecordOnlyCapture: false),
                .openRide(.vescOnewheel)
            )

            driver.onPhaseChange?(phase)

            XCTAssertEqual(model.phase, phase)
            XCTAssertEqual(model.connectionState, .picker)
            XCTAssertEqual(
                model.connectionState.navigationIntent(isRecordOnlyCapture: false),
                .returnToPicker
            )
            defaults.removePersistentDomain(forName: "CutoutAppModelTests.bluetoothLoss")
        }
    }

    @MainActor
    func testLateRetryCannotOverwriteADifferentSelectedDevice() {
        let vesc = DevicePickerRow(
            id: "vesc-1234",
            title: "VESC",
            subtitle: "VESC Onewheel",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .vescOnewheel
        )
        let euc = DevicePickerRow(
            id: "euc-5678",
            title: "EUC",
            subtitle: "Electric unicycle",
            detail: "Device 5678",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .electricUnicycle
        )
        let driver = SessionDriverSpy(rows: [vesc, euc])
        let model = CutoutAppModel(core: driver)
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: vesc.id))
        XCTAssertTrue(model.pair(platformIdentifier: euc.id))

        driver.onReconnectScheduled?(
            SessionConnectionRetry(
                platformIdentifier: vesc.id,
                attempt: 1,
                deadline: MonotonicMilliseconds(0),
                failure: .connectFailed("timed out")
            )
        )

        XCTAssertEqual(
            model.connectionState,
            .connecting(
                ConnectionSelection(
                    platformIdentifier: euc.id,
                    title: euc.title,
                    route: .electricUnicycle
                ),
                phase: .discoveringServices
            )
        )
    }

    func testConnectionStateOwnsNavigationIntent() {
        let selection = ConnectionSelection(
            platformIdentifier: "vesc-1234",
            title: "VESC",
            route: .vescOnewheel
        )

        XCTAssertEqual(
            ConnectionState.connected(selection).navigationIntent(isRecordOnlyCapture: false),
            .openRide(.vescOnewheel)
        )
        XCTAssertEqual(
            ConnectionState.retrying(
                selection,
                retry: SessionConnectionRetry(
                    platformIdentifier: selection.platformIdentifier,
                    attempt: 1,
                    deadline: MonotonicMilliseconds(0),
                    failure: .connectFailed("timed out")
                )
            ).navigationIntent(isRecordOnlyCapture: false),
            .stay
        )
        XCTAssertEqual(
            ConnectionState.failed(selection, .connectFailed("timed out")).navigationIntent(isRecordOnlyCapture: false),
            .returnToPicker
        )
        XCTAssertEqual(
            ConnectionState.picker.navigationIntent(isRecordOnlyCapture: false),
            .returnToPicker
        )
        XCTAssertEqual(
            ConnectionState.connected(selection).navigationIntent(isRecordOnlyCapture: true),
            .stay
        )
    }

    func testConnectionStateOwnsSelectedDeviceStatusText() {
        let selection = ConnectionSelection(
            platformIdentifier: "vesc-1234",
            title: "VESC",
            route: .vescOnewheel
        )
        let failure = SessionConnectionFailure.connectFailed("timed out")

        XCTAssertEqual(
            ConnectionState.connecting(selection, phase: .discoveringServices).statusText,
            SessionConnectionPhase.discoveringServices.displayText
        )
        XCTAssertEqual(
            ConnectionState.retrying(
                selection,
                retry: SessionConnectionRetry(
                    platformIdentifier: selection.platformIdentifier,
                    attempt: 1,
                    deadline: MonotonicMilliseconds(0),
                    failure: failure
                )
            ).statusText,
            localizedAppText("picker.status.retrying")
        )
        XCTAssertEqual(
            ConnectionState.connected(selection).statusText,
            SessionConnectionPhase.live.displayText
        )
        XCTAssertEqual(
            ConnectionState.failed(selection, failure).statusText,
            SessionConnectionPhase.failed(failure).displayText
        )
        XCTAssertNil(ConnectionState.picker.statusText)
    }

    @MainActor
    func testRecordOnlyCaptureKeepsTheRememberedDevice() {
        let store = DevicePickerSelectionStore()
        store.save(platformIdentifier: "saved-device")
        defer { clear(store) }
        let model = CutoutAppModel(core: SessionDriverSpy(rows: []))

        XCTAssertTrue(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: "Unknown device"))

        XCTAssertEqual(store.platformIdentifier, "saved-device")
        XCTAssertTrue(model.hasSavedDevice)
        XCTAssertEqual(model.connectionState.navigationIntent(isRecordOnlyCapture: true), .stay)
    }

    @MainActor
    func testFinishCaptureFlushesOnceBeforeDisconnecting() async {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)

        XCTAssertTrue(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: "Unknown device"))
        let firstFinishSucceeded = await model.finishCapture()
        let secondFinishSucceeded = await model.finishCapture()
        XCTAssertTrue(firstFinishSucceeded)
        XCTAssertFalse(secondFinishSucceeded)

        XCTAssertEqual(driver.flushCaptureCount, 1)
        XCTAssertEqual(driver.disconnectCount, 1)
    }

    @MainActor
    func testFinishCaptureKeepsTheCaptureRouteWhenWriterFlushFails() async {
        let driver = SessionDriverSpy(rows: [], flushSucceeds: false)
        let model = CutoutAppModel(core: driver)

        XCTAssertTrue(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: "Unknown device"))
        let finishSucceeded = await model.finishCapture()
        XCTAssertFalse(finishSucceeded)

        XCTAssertEqual(driver.flushCaptureCount, 1)
        XCTAssertEqual(driver.disconnectCount, 0)
        XCTAssertEqual(model.captureStatus, .failed)
    }

    @MainActor
    func testFlushFailureMarksOnlyAnActiveCaptureFailed() async {
        let driver = SessionDriverSpy(rows: [], flushSucceeds: false)
        let model = CutoutAppModel(core: driver)

        let inactiveFlushSucceeded = await model.flushCapture()
        XCTAssertFalse(inactiveFlushSucceeded)
        XCTAssertNil(model.captureStatus)

        model.applyCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/capture.jsonl")))
        let activeFlushSucceeded = await model.flushCapture()
        XCTAssertFalse(activeFlushSucceeded)
        XCTAssertEqual(model.captureStatus, .failed)
        XCTAssertEqual(driver.flushCaptureCount, 2)
    }

    @MainActor
    func testSceneBackgroundExecutesRustRequestedCaptureFlushOncePerTransition() async {
        let fixture = CutoutUITestSessionFixture.vesc
        let driver = SessionDriverSpy(rows: [fixture.candidate.pickerRow])
        let model = CutoutAppModel(core: driver, liveActivityManager: FailingLiveActivityManager(error: nil))
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: fixture.candidate.platformIdentifier))
        driver.onDisplayStateChange?(
            RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(100),
                    speed: Speed(value: 8_000),
                    operatingState: .riding
                )
            )
        )
        driver.onPhaseChange?(.subscribing)
        driver.onPhaseChange?(.live)
        for _ in 0 ..< 20 {
            if driver.rideSessionStateHandle.rideSessionSnapshot().phase == .active { break }
            await Task.yield()
        }

        model.appDidEnterBackground()
        for _ in 0 ..< 20 {
            if driver.flushCaptureCount == 1 { break }
            await Task.yield()
        }
        model.appDidEnterBackground()
        for _ in 0 ..< 5 { await Task.yield() }

        XCTAssertEqual(driver.flushCaptureCount, 1)
        XCTAssertEqual(driver.rideSessionStateHandle.rideSessionSnapshot().appPresence, .background)

        model.appDidBecomeActive()
        for _ in 0 ..< 20 {
            if driver.rideSessionStateHandle.rideSessionSnapshot().appPresence == .foreground { break }
            await Task.yield()
        }
        model.appDidEnterBackground()
        for _ in 0 ..< 20 {
            if driver.flushCaptureCount == 2 { break }
            await Task.yield()
        }

        XCTAssertEqual(driver.flushCaptureCount, 2)
        XCTAssertEqual(driver.rideSessionStateHandle.rideSessionSnapshot().appPresence, .background)
    }

    @MainActor
    func testRecordOnlyCaptureStillFlushesWhenTheAppEntersBackground() async {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)
        XCTAssertTrue(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: "Unknown device"))

        model.appDidEnterBackground()
        for _ in 0 ..< 20 {
            if driver.flushCaptureCount == 1 { break }
            await Task.yield()
        }

        XCTAssertEqual(driver.flushCaptureCount, 1)
    }

    func testUITestFixtureMarksCaptureFinalizationFailure() {
        XCTAssertFalse(CutoutUITestSessionFixture.unknownDeviceFinishFailure.flushCaptureSucceeds)
    }

    @MainActor
    func testDisconnectIgnoresLateConnectionCallbacks() {
        let row = DevicePickerRow(
            id: "vesc-1234",
            title: "VESC",
            subtitle: "VESC Onewheel",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .vescOnewheel
        )
        let driver = SessionDriverSpy(rows: [row])
        driver.protocolIdentityCandidate = DevicePickerDiscoveryCandidate(
            platformIdentifier: row.id,
            displayName: row.title,
            productCategory: row.subtitle,
            evidence: "advertisement",
            detail: row.detail,
            support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
            symbolName: row.symbolName
        )
        let model = CutoutAppModel(core: driver)
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: row.id))

        model.disconnectTransport()
        XCTAssertEqual(model.phase, .scanning)
        driver.onPhaseChange?(.scanning)
        driver.onPhaseChange?(.discoveringServices)
        driver.onReconnectScheduled?(
            SessionConnectionRetry(
                platformIdentifier: row.id,
                attempt: 1,
                deadline: MonotonicMilliseconds(0),
                failure: .connectFailed("timed out")
            )
        )
        driver.onPhaseChange?(.live)
        driver.onPhaseChange?(.failed(.connectFailed("timed out")))

        XCTAssertEqual(model.phase, .scanning)
        XCTAssertNil(model.selectedRideTitle)
        XCTAssertNil(model.selectedConnectionRoute)
    }

    func testCaptureStatusAnnouncesOnlyMeaningfulTransitions() {
        XCTAssertEqual(
            CaptureStatus.labelStarted(label: "Ride", notificationCount: 3, fileName: "ride.cutout")
                .accessibilityAnnouncement,
            "Ride capture started"
        )
        XCTAssertEqual(
            CaptureStatus.labelStopped(label: "Ride", notificationCount: 4, fileName: "ride.cutout")
                .accessibilityAnnouncement,
            "Ride capture stopped"
        )
        XCTAssertEqual(CaptureStatus.saved(fileName: "ride.cutout").accessibilityAnnouncement, "Capture saved")
        XCTAssertEqual(CaptureStatus.failed.accessibilityAnnouncement, "Capture failed")
        XCTAssertNil(
            CaptureStatus.recording(label: "Ride", notificationCount: 200, fileName: "ride.cutout")
                .accessibilityAnnouncement
        )
        XCTAssertNil(CaptureStatus.recordingLocally(fileName: "ride.cutout").accessibilityAnnouncement)
    }

    func testCaptureStatusPreservesVisibleLifecycleText() {
        XCTAssertEqual(
            CaptureStatus.recordingLocally(fileName: "ride.cutout").displayText,
            "Recording locally: ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.recording(label: nil, notificationCount: 2, fileName: nil).displayText,
            "Recording: 2 notifications"
        )
        XCTAssertEqual(
            CaptureStatus.recording(label: "Ride", notificationCount: 3, fileName: "ride.cutout").displayText,
            "Ride: 3 notifications → ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.labelStarted(label: "Ride", notificationCount: 3, fileName: "ride.cutout").displayText,
            "Ride started: 3 notifications → ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.labelStopped(label: "Ride", notificationCount: 4, fileName: nil).displayText,
            "Ride stopped"
        )
        XCTAssertEqual(
            CaptureStatus.saved(fileName: "ride.cutout").displayText,
            "Saved capture: ride.cutout"
        )
    }

    func testCaptureStatusMarksFinalizationFailure() {
        XCTAssertEqual(CaptureStatus.failed.statusStripTone, .critical)
        XCTAssertEqual(CaptureStatus.recordingLocally(fileName: "capture.jsonl").statusStripTone, .nominal)
    }

    @MainActor
    func testCaptureStatusConsumesTypedCoreEvents() {
        let model = CutoutAppModel()
        let fileURL = URL(fileURLWithPath: "/tmp/ride.cutout")

        model.applyCaptureEvent(.started(fileURL: fileURL))
        XCTAssertEqual(model.captureStatus, .recordingLocally(fileName: "ride.cutout"))

        model.applyCaptureEvent(.notificationRecorded)
        XCTAssertEqual(
            model.captureStatus,
            .recording(label: nil, notificationCount: 1, fileName: "ride.cutout")
        )

        model.applyCaptureEvent(.finished(fileURL: fileURL))
        XCTAssertEqual(model.captureStatus, .saved(fileName: "ride.cutout"))

        model.applyCaptureEvent(.failed)
        XCTAssertEqual(model.captureStatus, .failed)
    }

    @MainActor
    func testCaptureProgressPreservesWriterHealthAndFileMetadata() {
        let model = CutoutAppModel()
        let fileURL = URL(fileURLWithPath: "/tmp/ride.cutout")
        let progress = CaptureProgress(
            elapsedMilliseconds: 63_000,
            notificationCount: 42,
            fileSizeBytes: 12_288,
            queuedMessageCount: 2,
            writerError: nil
        )

        model.applyCaptureEvent(.started(fileURL: fileURL))
        model.applyCaptureEvent(.progress(progress))

        XCTAssertEqual(model.captureProgress, progress)
        XCTAssertEqual(
            model.captureStatus,
            .recording(label: nil, notificationCount: 42, fileName: "ride.cutout")
        )
    }

    @MainActor
    func testWriterProgressDoesNotRepublishAnUnchangedRideCaptureSummary() {
        let model = CutoutAppModel()
        let fileURL = URL(fileURLWithPath: "/tmp/ride.cutout")
        let initial = CaptureProgress(
            elapsedMilliseconds: 1_000,
            notificationCount: 42,
            fileSizeBytes: 4_096,
            queuedMessageCount: 2,
            writerError: nil
        )
        let updated = CaptureProgress(
            elapsedMilliseconds: 2_000,
            notificationCount: 42,
            fileSizeBytes: 8_192,
            queuedMessageCount: 1,
            writerError: nil
        )

        model.applyCaptureEvent(.started(fileURL: fileURL))
        model.applyCaptureEvent(.progress(initial))

        XCTAssertFalse(observesChange({ _ = model.captureStatusText }) {
            model.applyCaptureEvent(.progress(updated))
        })
        XCTAssertEqual(model.captureProgress, updated)

        let visibleSummaryChange = CaptureProgress(
            elapsedMilliseconds: 2_100,
            notificationCount: 43,
            fileSizeBytes: 8_384,
            queuedMessageCount: 1,
            writerError: nil
        )
        XCTAssertTrue(observesChange({ _ = model.captureStatusText }) {
            model.applyCaptureEvent(.progress(visibleSummaryChange))
        })
    }

    func testCaptureSessionDetailsExposeTypedWriterHealth() {
        let healthyProgress = CaptureProgress(
            elapsedMilliseconds: 63_000,
            notificationCount: 42,
            fileSizeBytes: 12_288,
            queuedMessageCount: 0,
            writerError: nil
        )
        let failedProgress = CaptureProgress(
            elapsedMilliseconds: 63_000,
            notificationCount: 42,
            fileSizeBytes: 12_288,
            queuedMessageCount: 0,
            writerError: "queue overrun"
        )
        let healthyRows = captureSessionDetailRows(progress: healthyProgress)
        let failedRows = captureSessionDetailRows(progress: failedProgress)

        XCTAssertEqual(healthyProgress.writerHealth, .healthy)
        XCTAssertEqual(failedProgress.writerHealth, .failed)
        XCTAssertEqual(healthyRows[0].metricValue, healthyProgress.elapsedMetricValue)
        XCTAssertEqual(healthyRows[1].metricValue, healthyProgress.notificationCountMetricValue)
        XCTAssertEqual(healthyRows[2].metricValue, healthyProgress.fileSizeMetricValue)
        XCTAssertEqual(healthyRows[3].metricValue, healthyProgress.queuedMessageCountMetricValue)
        XCTAssertEqual(
            healthyRows[4].metricValue,
            healthyProgress.writerHealth.metricValue(display: "Healthy")
        )
        XCTAssertEqual(
            failedRows[4].metricValue,
            failedProgress.writerHealth.metricValue(display: "Failed")
        )

        XCTAssertEqual(
            healthyRows.map(\.id),
            [
                "capture-elapsed",
                "capture-packets",
                "capture-file-size",
                "capture-queued-messages",
                "capture-writer-health",
            ]
        )
        XCTAssertEqual(healthyRows[3].label, "Pending writes")
        XCTAssertEqual(healthyRows[3].accessibilityValueText, "0")
        XCTAssertEqual(healthyRows.last?.label, "Writer")
        XCTAssertEqual(healthyRows.last?.accessibilityValueText, "Healthy")
        XCTAssertEqual(failedRows.last?.accessibilityValueText, "Failed")
    }

    @MainActor
    func testNewRecordOnlyCaptureClearsPriorSessionStatusBeforeCoreEvents() {
        let model = CutoutAppModel(core: SessionDriverSpy(rows: []))
        let priorCapture = URL(fileURLWithPath: "/tmp/prior.cutout")
        let priorProgress = Self.priorCaptureProgress

        model.applyCaptureEvent(.started(fileURL: priorCapture))
        model.applyCaptureEvent(.progress(priorProgress))
        model.startCaptureLabel(.ride)
        XCTAssertEqual(model.captureStatus, .labelStarted(label: "Ride", notificationCount: 42, fileName: "prior.cutout"))
        XCTAssertEqual(model.captureProgress, priorProgress)
        XCTAssertEqual(model.activeCaptureLabels, [.ride])

        XCTAssertFalse(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: " "))
        XCTAssertEqual(model.captureStatus, .labelStarted(label: "Ride", notificationCount: 42, fileName: "prior.cutout"))
        XCTAssertEqual(model.captureProgress, priorProgress)
        XCTAssertEqual(model.activeCaptureLabels, [.ride])

        XCTAssertTrue(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: "Unknown device"))

        XCTAssertNil(model.captureStatus)
        XCTAssertNil(model.captureStatusText)
        XCTAssertNil(model.captureProgress)
        XCTAssertTrue(model.activeCaptureLabels.isEmpty)
    }

    @MainActor
    func testRejectedRecordOnlyCapturePreservesTheExistingSession() {
        let model = CutoutAppModel()
        let priorCapture = URL(fileURLWithPath: "/tmp/prior.cutout")
        let priorProgress = Self.priorCaptureProgress

        model.applyCaptureEvent(.started(fileURL: priorCapture))
        model.applyCaptureEvent(.progress(priorProgress))
        model.startCaptureLabel(.ride)

        XCTAssertFalse(model.recordOnly(platformIdentifier: "missing-device", deviceKind: "Unknown device"))

        XCTAssertEqual(model.captureStatus, .labelStarted(label: "Ride", notificationCount: 42, fileName: "prior.cutout"))
        XCTAssertEqual(model.captureProgress, priorProgress)
        XCTAssertEqual(model.activeCaptureLabels, [.ride])
    }

    @MainActor
    func testCaptureLabelActionsIgnoreInvalidRepeatedTransitions() {
        let model = CutoutAppModel()

        model.stopCaptureLabel(.ride)
        XCTAssertNil(model.captureStatusText)
        XCTAssertTrue(model.activeCaptureLabels.isEmpty)

        model.startCaptureLabel(.ride)
        XCTAssertEqual(model.captureStatusText, "Ride started")
        XCTAssertEqual(model.activeCaptureLabels, [.ride])

        model.stopCaptureLabel(.ride)
        XCTAssertEqual(model.captureStatusText, "Ride stopped")
        XCTAssertTrue(model.activeCaptureLabels.isEmpty)

        model.stopCaptureLabel(.ride)
        XCTAssertEqual(model.captureStatusText, "Ride stopped")
    }

    @MainActor
    func testCaptureLabelActionsReplaceAnActiveExclusiveMode() {
        let transitions: [(active: CaptureQuickLabel, replacement: CaptureQuickLabel)] = [
            (CaptureQuickLabel.lowBeamOn, .lowBeamOff),
            (.highBeamOn, .highBeamOff),
            (.pedalsHard, .pedalsSoft),
            (.softwareLock, .softwareUnlock),
        ]

        for (active, replacement) in transitions {
            let driver = SessionDriverSpy(rows: [])
            let model = CutoutAppModel(core: driver)

            model.startCaptureLabel(active)
            model.startCaptureLabel(replacement)

            XCTAssertEqual(model.activeCaptureLabels, [replacement])
            XCTAssertEqual(model.captureStatusText, "\(replacement.title) started")
            XCTAssertEqual(
                driver.captureAnnotations,
                [
                    "\(active.annotationValue)_start",
                    "\(active.annotationValue)_stop",
                    "\(replacement.annotationValue)_start",
                ]
            )
        }
    }

    @MainActor
    func testProtocolIdentityCandidateDoesNotOverwriteSelectedRideTitle() {
        let model = CutoutAppModel()
        let supported = DevicePickerCandidateSupport.supported(
            connectionRoute: .vescOnewheel,
            electricUnicycleModel: nil
        )
        let first = DevicePickerDiscoveryCandidate(
            platformIdentifier: "vesc-1",
            displayName: "Little FOCer BT",
            productCategory: "VESC Onewheel",
            evidence: "advertisement",
            detail: "Little FOCer BT",
            support: supported,
            symbolName: "circle.hexagongrid.circle"
        )
        let second = DevicePickerDiscoveryCandidate(
            platformIdentifier: "vesc-1",
            displayName: "VESC stream",
            productCategory: "VESC Onewheel",
            evidence: "advertisement",
            detail: "VESC device",
            support: supported,
            symbolName: "circle.hexagongrid.circle"
        )

        model.applyProtocolIdentityCandidate(first)
        XCTAssertEqual(model.selectedRideTitle, "Little FOCer BT")

        model.applyProtocolIdentityCandidate(second)
        XCTAssertEqual(model.selectedRideTitle, "Little FOCer BT")
    }

    @MainActor
    func testRideMapVehicleNameRematchesTheCurrentProtocolIdentity() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)
        let supported = DevicePickerCandidateSupport.supported(
            connectionRoute: .electricUnicycle,
            electricUnicycleModel: .aero
        )
        let persistedName = DevicePickerDiscoveryCandidate(
            platformIdentifier: "wheel-1",
            displayName: "wheel-1",
            productCategory: "Electric unicycle",
            evidence: "restored selection",
            detail: "restored selection",
            support: supported,
            symbolName: "circle.hexagongrid.circle"
        )
        let protocolIdentity = DevicePickerDiscoveryCandidate(
            platformIdentifier: "wheel-1",
            displayName: "NF2557",
            productCategory: "Electric unicycle",
            evidence: "Veteran protocol model id",
            detail: "NOSFET Aero confirmed by model id 43",
            support: supported,
            symbolName: "circle.hexagongrid.circle"
        )
        driver.protocolIdentityCandidate = protocolIdentity

        model.applyProtocolIdentityCandidate(persistedName)
        XCTAssertEqual(model.selectedRideTitle, "wheel-1")
        model.applyProtocolIdentityCandidate(protocolIdentity)

        XCTAssertEqual(model.selectedRideTitle, "wheel-1")
        XCTAssertEqual(model.rideMapVehicleIdentity, "wheel-1")
        XCTAssertEqual(model.rideMapVehicleName, "NF2557")
    }

    @MainActor
    func testProtocolIdentityCandidatePersistsNamesForHistoryIdentities() throws {
        let suiteName = "CutoutAppModelTests.\(#function).\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = DevicePickerSelectionStore(defaults: defaults)
        let model = CutoutAppModel(core: SessionDriverSpy(rows: []), selectedDeviceStore: store)
        let supported = DevicePickerCandidateSupport.supported(
            connectionRoute: .electricUnicycle,
            electricUnicycleModel: .aero
        )
        let candidate = DevicePickerDiscoveryCandidate(
            platformIdentifier: "old-core-bluetooth-id",
            displayName: "NF2557",
            productCategory: "Electric unicycle",
            evidence: "advertisement",
            detail: "resolved device",
            support: supported,
            symbolName: "circle.hexagongrid.circle"
        )

        model.applyProtocolIdentityCandidate(candidate)

        XCTAssertEqual(store.displayName(for: "old-core-bluetooth-id"), "NF2557")
    }

    @MainActor
    func testRideMapVehicleNameNeverTreatsThePlatformIdentityAsDisplayName() {
        XCTAssertNil(CutoutAppModel.meaningfulDeviceName("wheel-1", identity: "wheel-1"))
        XCTAssertNil(CutoutAppModel.meaningfulDeviceName("", identity: "wheel-1"))
        XCTAssertEqual(CutoutAppModel.meaningfulDeviceName("NF2557", identity: "wheel-1"), "NF2557")
    }

    @MainActor
    func testLiveActivityStartFailureIsObservableAndRetryable() async {
        let row = DevicePickerRow(
            id: "vesc-1234",
            title: "VESC",
            subtitle: "VESC Onewheel",
            detail: "Device 1234",
            state: DevicePickerRowState(action: .use),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .vescOnewheel
        )
        let driver = SessionDriverSpy(rows: [row])
        let manager = FailingLiveActivityManager(error: .authorizationDenied)
        let model = CutoutAppModel(
            core: driver,
            liveActivityManager: manager
        )
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: row.id))

        for _ in 0 ..< 20 {
            if model.liveActivityError != nil { break }
            try? await Task.sleep(for: .milliseconds(1))
        }

        XCTAssertEqual(model.liveActivityError, .authorizationDenied)

        await manager.setError(nil)
        XCTAssertTrue(model.pair(platformIdentifier: row.id))
        XCTAssertFalse(model.pair(platformIdentifier: row.id))

        for _ in 0 ..< 20 {
            if await manager.startCount == 2 { break }
            try? await Task.sleep(for: .milliseconds(1))
        }

        let startCount = await manager.startCount
        XCTAssertEqual(startCount, 2)
        XCTAssertNil(model.liveActivityError)
    }

    func testLaunchArgumentFixtureOverridesPersistentFixture() {
        let fixture = CutoutUITestSessionFixture.resolve(
            persistedValue: "vesc-live-activity-auto",
            arguments: ["-CUTOUT_UI_TEST_FIXTURE", "euc"]
        )

        XCTAssertFalse(fixture?.startsLive ?? true)
        XCTAssertTrue(fixture?.isEuc ?? false)
    }

    func testLaunchEnvironmentFixtureSurvivesMissingProcessArgument() {
        let fixture = CutoutUITestSessionFixture.resolve(
            environmentValue: "euc",
            persistedValue: nil,
            arguments: []
        )

        XCTAssertTrue(fixture?.isEuc ?? false)
    }

    func testCriticalLiveActivityFixtureStartsLiveWithCriticalPwm() {
        let fixture = CutoutUITestSessionFixture.resolve(
            persistedValue: nil,
            arguments: ["-CUTOUT_UI_TEST_FIXTURE", "vesc-live-activity-critical-auto"]
        )

        XCTAssertTrue(fixture?.startsLive ?? false)
        XCTAssertEqual(fixture?.testScript.telemetry?.pwm?.permille, 850)
    }

    func testUnavailableLiveActivityFixtureStartsLiveWithoutTelemetry() {
        let fixture = CutoutUITestSessionFixture(value: "vesc-live-activity-unavailable-auto")

        XCTAssertTrue(fixture?.startsLive ?? false)
        XCTAssertTrue(fixture?.emitsPendingTelemetry ?? false)
        XCTAssertNil(fixture?.testScript.telemetry)
    }

    func testStaleLiveActivityFixtureStartsLiveWithStaleTelemetry() {
        let fixture = CutoutUITestSessionFixture(value: "vesc-live-activity-stale-auto")

        XCTAssertTrue(fixture?.startsLive ?? false)
        XCTAssertTrue(fixture?.emitsStaleTelemetry ?? false)
        XCTAssertNotNil(fixture?.testScript.telemetry)
    }

    func testBackgroundLiveActivityFixtureDelaysAnIndependentTelemetryUpdate() {
        let fixture = CutoutUITestSessionFixture(value: "vesc-live-activity-dynamic-auto")

        XCTAssertTrue(fixture?.startsLive ?? false)
        XCTAssertEqual(fixture?.testScript.telemetry?.speed, Speed(value: 8_000))
        XCTAssertEqual(fixture?.testScript.telemetryUpdate?.speed, Speed(value: 16_000))
        XCTAssertEqual(fixture?.testScript.telemetryUpdateDelayMilliseconds, 8_000)
    }

    @MainActor
    func testAutoLiveActivityUsesTheCandidateDisplayName() async {
        let fixture = CutoutUITestSessionFixture.autoVescLiveActivity
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(
            core: CutoutSessionCore(testScript: fixture.testScript),
            liveActivityManager: manager
        )

        model.start()
        for _ in 0 ..< 200 {
            if await manager.lastStartedSnapshot != nil { break }
            await Task.yield()
        }

        let snapshot = await manager.lastStartedSnapshot
        XCTAssertEqual(snapshot?.identity.label, fixture.candidate.displayName)
    }

    @MainActor
    func testBluetoothUnavailableUsesTheTypedRustTerminalReason() async {
        let fixture = CutoutUITestSessionFixture.vesc
        let driver = SessionDriverSpy(rows: [fixture.candidate.pickerRow])
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(core: driver, liveActivityManager: manager)
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: fixture.candidate.platformIdentifier))
        driver.onPhaseChange?(.subscribing)
        driver.onPhaseChange?(.live)
        for _ in 0 ..< 20 {
            if driver.rideSessionStateHandle.rideSessionSnapshot().phase == .active { break }
            await Task.yield()
        }
        driver.onPhaseChange?(.bluetoothUnavailable(rawState: 4))

        for _ in 0 ..< 200 {
            if case .ended(reason: .unrecoverableSessionFailure) = driver.rideSessionStateHandle.rideSessionSnapshot().phase {
                break
            }
            await Task.yield()
        }

        let endReason = await manager.lastEndReason
        XCTAssertEqual(
            driver.rideSessionStateHandle.rideSessionSnapshot().phase,
            .ended(reason: .unrecoverableSessionFailure)
        )
        XCTAssertEqual(endReason, .unavailable)
    }

    @MainActor
    func testExhaustedConnectionFailureUsesTheTypedRustTerminalReason() async {
        let fixture = CutoutUITestSessionFixture.vesc
        let driver = SessionDriverSpy(rows: [fixture.candidate.pickerRow])
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(core: driver, liveActivityManager: manager)
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: fixture.candidate.platformIdentifier))
        driver.onDisplayStateChange?(
            RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(100),
                    speed: Speed(value: 8_000),
                    operatingState: .riding
                )
            )
        )
        driver.onPhaseChange?(.subscribing)
        driver.onPhaseChange?(.live)
        for _ in 0 ..< 200 {
            if driver.rideSessionStateHandle.rideSessionSnapshot().phase == .active { break }
            await Task.yield()
        }

        driver.onPhaseChange?(.failed(.connectFailed("retries exhausted")))
        for _ in 0 ..< 200 {
            if case .ended = driver.rideSessionStateHandle.rideSessionSnapshot().phase { break }
            await Task.yield()
        }

        let endReason = await manager.lastEndReason
        XCTAssertEqual(
            driver.rideSessionStateHandle.rideSessionSnapshot().phase,
            .ended(reason: .reconnectExhausted)
        )
        XCTAssertEqual(endReason, .unavailable)
    }

    @MainActor
    func testTransientReconnectKeepsTheLiveActivityStaleAndReusesItsIdentity() async {
        let suiteName = "CutoutAppModelTests.transientReconnect.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let fixture = CutoutUITestSessionFixture.vesc
        let driver = SessionDriverSpy(rows: [fixture.candidate.pickerRow])
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(
            core: driver,
            selectedDeviceStore: DevicePickerSelectionStore(defaults: defaults),
            rideSessionMarkerStore: RideSessionMarkerStore(defaults: defaults),
            liveActivityManager: manager
        )
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: fixture.candidate.platformIdentifier))
        driver.onDisplayStateChange?(
            RideDisplayState(
                speed: SpeedReadout(millimetersPerSecond: 8_000),
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(100),
                    speed: Speed(value: 8_000),
                    operatingState: .riding
                )
            )
        )
        driver.onPhaseChange?(.subscribing)
        driver.onPhaseChange?(.live)

        for _ in 0 ..< 200 {
            if await manager.lastStartedSnapshot?.connectionState == .connected,
               driver.rideSessionStateHandle.rideSessionSnapshot().phase == .active { break }
            await Task.yield()
        }
        let identity = driver.rideSessionStateHandle.rideSessionSnapshot().identity

        driver.onPhaseChange?(.discoveringServices)
        driver.onReconnectScheduled?(
            SessionConnectionRetry(
                platformIdentifier: fixture.candidate.platformIdentifier,
                attempt: 1,
                deadline: MonotonicMilliseconds(200),
                failure: .connectFailed("timed out")
            )
        )
        for _ in 0 ..< 200 {
            if driver.rideSessionStateHandle.rideSessionSnapshot().phase == .reconnecting,
               await manager.lastUpdatedSnapshot?.connectionState == .stale { break }
            await Task.yield()
        }

        let endReason = await manager.lastEndReason
        let updatedConnectionState = await manager.lastUpdatedSnapshot?.connectionState
        XCTAssertNil(endReason)
        XCTAssertEqual(driver.rideSessionStateHandle.rideSessionSnapshot().identity, identity)
        XCTAssertEqual(driver.rideSessionStateHandle.rideSessionSnapshot().phase, .reconnecting)
        XCTAssertEqual(updatedConnectionState, .stale)

        driver.onPhaseChange?(.subscribing)
        driver.onDisplayStateChange?(
            RideDisplayState(
                speed: SpeedReadout(millimetersPerSecond: 9_000),
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(300),
                    speed: Speed(value: 9_000),
                    operatingState: .riding
                )
            )
        )
        driver.onPhaseChange?(.live)
        for _ in 0 ..< 200 {
            if driver.rideSessionStateHandle.rideSessionSnapshot().phase == .active { break }
            await Task.yield()
        }

        let startCount = await manager.startCount
        let reconnectEndReason = await manager.lastEndReason
        XCTAssertEqual(startCount, 1)
        XCTAssertNil(reconnectEndReason)
        XCTAssertEqual(driver.rideSessionStateHandle.rideSessionSnapshot().identity, identity)
        XCTAssertEqual(driver.rideSessionStateHandle.rideSessionSnapshot().phase, .active)
    }

    @MainActor
    func testRestoredSelectedPeripheralReusesThePersistedRustRideIdentity() async throws {
        let suiteName = "CutoutAppModelTests.restoredRide.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let selectedDeviceStore = DevicePickerSelectionStore(defaults: defaults)
        let fixture = CutoutUITestSessionFixture.vesc
        let platformIdentifier = fixture.candidate.platformIdentifier
        selectedDeviceStore.save(platformIdentifier: platformIdentifier)
        let source = CutoutSessionStateHandle()
        let started = try source.reduceRideSession(
            input: .start(platformIdentifier: platformIdentifier)
        )
        markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
        let driver = SessionDriverSpy(
            rows: [fixture.candidate.pickerRow],
            restoredPlatformIdentifier: platformIdentifier
        )
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(
            core: driver,
            selectedDeviceStore: selectedDeviceStore,
            rideSessionMarkerStore: markerStore,
            liveActivityManager: manager
        )

        model.start()
        for _ in 0 ..< 50 {
            if driver.rideSessionStateHandle.rideSessionSnapshot().phase == .reconnecting { break }
            try? await Task.sleep(for: .milliseconds(1))
        }
        driver.onPhaseChange?(.subscribing)
        driver.onPhaseChange?(.live)
        for _ in 0 ..< 50 {
            if driver.rideSessionStateHandle.rideSessionSnapshot().phase == .active { break }
            try? await Task.sleep(for: .milliseconds(1))
        }

        XCTAssertEqual(
            driver.rideSessionStateHandle.rideSessionSnapshot().identity,
            started.snapshot.identity
        )
        XCTAssertEqual(model.phase, .live)
        XCTAssertEqual(driver.rideSessionStateHandle.rideSessionSnapshot().phase, .active)
        let startCount = await manager.startCount
        XCTAssertEqual(startCount, 1)
        XCTAssertNotNil(markerStore.marker)
    }

    @MainActor
    func testRestoredPeripheralWithoutAPersistedRideRequiresUserAction() async throws {
        let suiteName = "CutoutAppModelTests.unmarkedRestoredRide.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let selectedDeviceStore = DevicePickerSelectionStore(defaults: defaults)
        let fixture = CutoutUITestSessionFixture.vesc
        let platformIdentifier = fixture.candidate.platformIdentifier
        selectedDeviceStore.save(platformIdentifier: platformIdentifier)
        let driver = SessionDriverSpy(
            rows: [fixture.candidate.pickerRow],
            restoredPlatformIdentifier: platformIdentifier
        )
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(
            core: driver,
            selectedDeviceStore: selectedDeviceStore,
            rideSessionMarkerStore: RideSessionMarkerStore(defaults: defaults),
            liveActivityManager: manager
        )

        model.start()
        try? await Task.sleep(for: .milliseconds(10))

        XCTAssertEqual(driver.pairedPlatformIdentifiers, [])
        let startCount = await manager.startCount
        XCTAssertEqual(startCount, 0)
        XCTAssertTrue(model.pair(platformIdentifier: platformIdentifier))
    }

    @MainActor
    func testLateRestorationCallbackCannotReinterpretAnExplicitPairAsRestored() async throws {
        let suiteName = "CutoutAppModelTests.lateRestoration.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let selectedDeviceStore = DevicePickerSelectionStore(defaults: defaults)
        let fixture = CutoutUITestSessionFixture.vesc
        let platformIdentifier = fixture.candidate.platformIdentifier
        let driver = SessionDriverSpy(
            rows: [fixture.candidate.pickerRow],
            restoredPlatformIdentifier: platformIdentifier,
            notifyBluetoothRestorationOnStart: false
        )
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(
            core: driver,
            selectedDeviceStore: selectedDeviceStore,
            rideSessionMarkerStore: markerStore,
            liveActivityManager: manager
        )

        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: platformIdentifier))
        for _ in 0 ..< 200 {
            if markerStore.marker != nil { break }
            await Task.yield()
        }

        driver.onBluetoothRestorationResolved?(platformIdentifier)
        try? await Task.sleep(for: .milliseconds(10))

        XCTAssertEqual(driver.pairedPlatformIdentifiers, [platformIdentifier])
        let endReason = await manager.lastEndReason
        XCTAssertNil(endReason)
    }

    @MainActor
    func testRestoredPeripheralDifferentFromPersistedRideRequiresUserAction() async throws {
        let suiteName = "CutoutAppModelTests.replacedRestoredRide.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let selectedDeviceStore = DevicePickerSelectionStore(defaults: defaults)
        let fixture = CutoutUITestSessionFixture.vesc
        let restoredPlatformIdentifier = fixture.candidate.platformIdentifier
        selectedDeviceStore.save(platformIdentifier: restoredPlatformIdentifier)
        let source = CutoutSessionStateHandle()
        _ = try source.reduceRideSession(input: .start(platformIdentifier: "previous-vesc"))
        markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
        let driver = SessionDriverSpy(
            rows: [fixture.candidate.pickerRow],
            restoredPlatformIdentifier: restoredPlatformIdentifier
        )
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(
            core: driver,
            selectedDeviceStore: selectedDeviceStore,
            rideSessionMarkerStore: markerStore,
            liveActivityManager: manager
        )

        model.start()
        for _ in 0 ..< 50 {
            if case .ended(reason: .appReset) = driver.rideSessionStateHandle.rideSessionSnapshot().phase {
                break
            }
            try? await Task.sleep(for: .milliseconds(1))
        }

        let endReason = await manager.lastEndReason
        let initialStartCount = await manager.startCount
        XCTAssertEqual(endReason, .sessionEnded)
        XCTAssertEqual(driver.pairedPlatformIdentifiers, [])
        XCTAssertEqual(driver.rideSessionStateHandle.rideSessionSnapshot().phase, .ended(reason: .appReset))
        XCTAssertEqual(initialStartCount, 0)
        XCTAssertNil(markerStore.marker)

        XCTAssertTrue(model.pair(platformIdentifier: restoredPlatformIdentifier))
        for _ in 0 ..< 50 {
            if await manager.startCount == 1 { break }
            try? await Task.sleep(for: .milliseconds(1))
        }
        let userActionStartCount = await manager.startCount
        XCTAssertEqual(userActionStartCount, 1)
    }

    @MainActor
    func testLaunchWithoutARestoredPeripheralEndsThePersistedRideAsAppReset() async throws {
        let suiteName = "CutoutAppModelTests.orphanRide.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let source = CutoutSessionStateHandle()
        _ = try source.reduceRideSession(input: .start(platformIdentifier: "vesc-platform-id"))
        markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
        let driver = SessionDriverSpy(rows: [], restoredPlatformIdentifier: nil)
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(
            core: driver,
            rideSessionMarkerStore: markerStore,
            liveActivityManager: manager
        )

        model.start()
        for _ in 0 ..< 50 {
            if case .ended = driver.rideSessionStateHandle.rideSessionSnapshot().phase { break }
            try? await Task.sleep(for: .milliseconds(1))
        }

        XCTAssertEqual(
            driver.rideSessionStateHandle.rideSessionSnapshot().phase,
            .ended(reason: .appReset)
        )
        let endReason = await manager.lastEndReason
        XCTAssertEqual(endReason, .sessionEnded)
        XCTAssertNil(markerStore.marker)
    }

    @MainActor
    func testScanningLaunchClearsAnOrphanedLiveActivity() async {
        let driver = SessionDriverSpy(rows: [])
        let manager = FailingLiveActivityManager(error: nil)
        let model = CutoutAppModel(core: driver, liveActivityManager: manager)

        model.start()
        driver.onPhaseChange?(.scanning)
        for _ in 0 ..< 20 {
            if await manager.lastEndReason != nil { break }
            await Task.yield()
        }

        let endReason = await manager.lastEndReason
        XCTAssertEqual(endReason, .disconnected)
    }

    func testStandardUIFixtureLaunchArgumentSelectsEucFixture() {
        let fixture = CutoutUITestSessionFixture.resolve(
            persistedValue: nil,
            arguments: ["-CUTOUT_UI_TEST_FIXTURE", "euc"]
        )

        XCTAssertTrue(fixture?.isEuc ?? false)
    }

    func testStandardUIFixtureLaunchArgumentSelectsBluetoothPermissionDeniedFixture() {
        let fixture = CutoutUITestSessionFixture.resolve(
            persistedValue: nil,
            arguments: ["-CUTOUT_UI_TEST_FIXTURE", "bluetooth-permission-denied"]
        )

        guard case .permissionDenied? = fixture?.initialBluetoothState else {
            return XCTFail("expected the permission-denied initial Bluetooth state")
        }
    }

    func testProbeTimeoutFixturePublishesTypedIdentificationFailure() {
        let fixture = CutoutUITestSessionFixture(value: "probe-timeout")

        XCTAssertEqual(fixture?.identificationProbeFailure, .timedOut)
        XCTAssertEqual(fixture?.candidate.support, .probeRecommended(disabledReason: "Identity probe required"))
    }

    func testProbeRefusalFixturesPublishTypedIdentificationFailures() {
        XCTAssertEqual(
            CutoutUITestSessionFixture(value: "probe-malformed")?.identificationProbeFailure,
            .malformedResponse
        )
        XCTAssertEqual(
            CutoutUITestSessionFixture(value: "probe-conflict")?.identificationProbeFailure,
            .conflictingEvidence
        )
        XCTAssertEqual(
            CutoutUITestSessionFixture(value: "probe-unsupported")?.identificationProbeFailure,
            .unsupported
        )
    }

    func testStandardUIFixtureLaunchArgumentSelectsEucOverviewFixture() {
        let fixture = CutoutUITestSessionFixture.resolve(
            persistedValue: nil,
            arguments: ["-CUTOUT_UI_TEST_FIXTURE", "euc-overview"]
        )

        XCTAssertEqual(fixture, .eucOverview)
        XCTAssertTrue(fixture?.isEuc ?? false)
    }

    func testStandardUIFixtureLaunchArgumentSurvivesRightToLeftSystemArguments() {
        let fixture = CutoutUITestSessionFixture.resolve(
            persistedValue: nil,
            arguments: [
                "-CUTOUT_UI_TEST_FIXTURE", "euc",
                "-AppleLanguages", "(ar)",
                "-AppleLocale", "ar_SA",
            ]
        )

        XCTAssertTrue(fixture?.isEuc ?? false)
    }

    func testPseudolocalizedStaleVescFixtureUsesTheDeterministicCoreScript() {
        let fixture = CutoutUITestSessionFixture.resolve(
            persistedValue: nil,
            arguments: [
                "-CUTOUT_UI_TEST_FIXTURE", "vesc-stale",
                "-NSDoubleLocalizedStrings", "YES",
                "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL",
            ]
        )

        XCTAssertEqual(fixture?.candidate.platformIdentifier, "ui-test-vesc")
        XCTAssertTrue(fixture?.emitsStaleTelemetry ?? false)
        XCTAssertFalse(fixture?.emitsPendingTelemetry ?? true)
        XCTAssertFalse(fixture?.startsLive ?? true)
    }

    func testRefloatSafetyAndModeFixturesPublishTypedRideState() {
        let warningFixtures: [(String, VescRideWarning)] = [
            ("vesc-low-voltage", .lowVoltage),
            ("vesc-high-voltage", .highVoltage),
            ("vesc-mosfet-temperature", .mosfetTemperature),
            ("vesc-motor-temperature", .motorTemperature),
            ("vesc-current", .current),
            ("vesc-duty-pushback", .dutyPushback),
            ("vesc-temperature-pushback", .temperaturePushback),
            ("vesc-wheelslip", .wheelslip),
            ("vesc-sensors", .sensors),
            ("vesc-low-battery", .lowBattery),
            ("vesc-error", .error),
        ]
        for (fixture, warning) in warningFixtures {
            XCTAssertEqual(
                CutoutUITestSessionFixture(value: fixture)?.testScript.telemetry?.vescWarning,
                warning
            )
        }
        let stopFixtures: [(String, VescRideStopReason)] = [
            ("vesc-pitch-stop", .pitch),
            ("vesc-roll-stop", .roll),
            ("vesc-switch-half-stop", .switchHalf),
            ("vesc-switch-full-stop", .switchFull),
            ("vesc-reverse-stop", .reverse),
            ("vesc-quick-stop", .quickStop),
        ]
        for (fixture, stopReason) in stopFixtures {
            let telemetry = CutoutUITestSessionFixture(value: fixture)?.testScript.telemetry
            XCTAssertEqual(telemetry?.vescStopReason, stopReason)
            XCTAssertEqual(telemetry?.vescWarning, VescRideWarning.none)
        }
        XCTAssertEqual(
            CutoutUITestSessionFixture(value: "vesc-handtest")?.testScript.telemetry?.vescOperatingMode,
            .handtest
        )
        XCTAssertEqual(
            CutoutUITestSessionFixture(value: "vesc-darkride")?.testScript.telemetry?.vescOperatingMode,
            .darkride
        )
        XCTAssertEqual(
            CutoutUITestSessionFixture(value: "vesc-flywheel")?.testScript.telemetry?.vescOperatingMode,
            .flywheel
        )
    }

    func testEucReconnectFixtureKeepsTheEucRoute() {
        let fixture = CutoutUITestSessionFixture(value: "euc-reconnect")

        XCTAssertTrue(fixture?.isEuc ?? false)
        XCTAssertTrue(fixture?.reconnectsAfterFirstLive ?? false)
    }

    func testBluetoothLossFixtureTransitionsAfterLive() {
        let fixture = CutoutUITestSessionFixture(value: "vesc-bluetooth-loss")

        XCTAssertEqual(fixture?.candidate.platformIdentifier, "ui-test-vesc")
        XCTAssertEqual(fixture?.testScript.bluetoothLossAfterFirstLiveMilliseconds, 1_500)
    }

    func testDynamicFixturesPublishASecondTelemetrySample() {
        let cases = [
            ("vesc-dynamic", Speed(value: 8_000), Speed(value: 16_000)),
            ("euc-dynamic", Speed(value: 12_000), Speed(value: 18_000)),
        ]

        for (name, initialSpeed, updatedSpeed) in cases {
            let fixture = CutoutUITestSessionFixture(value: name)
            XCTAssertEqual(fixture?.testScript.telemetry?.speed, initialSpeed)
            XCTAssertEqual(fixture?.testScript.telemetryUpdate?.speed, updatedSpeed)
            XCTAssertEqual(fixture?.testScript.telemetryUpdateDelayMilliseconds, 1_500)
        }
    }
}

@MainActor
private func observesChange(_ render: () -> Void, _ change: () -> Void) -> Bool {
    let invalidated = Mutex(false)
    withObservationTracking(render) {
        invalidated.withLock { $0 = true }
    }
    change()
    return invalidated.withLock { $0 }
}

private actor FailingLiveActivityManager: LiveActivityRideLifecycleManaging {
    private var error: LiveActivityRideLifecycleError?
    private(set) var startCount = 0
    private(set) var lastStartedSnapshot: LiveActivityRideSnapshot?
    private(set) var lastUpdatedSnapshot: LiveActivityRideSnapshot?
    private(set) var lastEndReason: LiveActivityRideLifecycleEndReason?

    init(error: LiveActivityRideLifecycleError?) {
        self.error = error
    }

    func start(
        snapshot: LiveActivityRideSnapshot,
        rideSessionIdentity _: LiveActivityRideSessionIdentity,
        staleAfterMilliseconds _: UInt64
    ) async throws -> LiveActivityRideStartOutcome {
        startCount += 1
        lastStartedSnapshot = snapshot
        if let error { throw error }
        return .started(activityID: "activity-1")
    }

    func update(
        snapshot: LiveActivityRideSnapshot,
        staleAfterMilliseconds _: UInt64
    ) async throws -> LiveActivityRideUpdateOutcome {
        lastUpdatedSnapshot = snapshot
        return LiveActivityRideUpdateOutcome(activityID: "activity-1")
    }

    func end(reason: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        lastEndReason = reason
        return LiveActivityRideEndOutcome(activityIDs: ["activity-1"])
    }

    func setError(_ error: LiveActivityRideLifecycleError?) {
        self.error = error
    }
}

@MainActor
private final class SessionDriverSpy: CutoutSessionDriving {
    let rideSessionStateHandle = CutoutSessionStateHandle()
    let rideMapStateHandle = MobileRideMapState()
    let rideMapStorageError: String?
    let rideMapAvailability: MobileRideMapAvailability = .ready
    var onDisplayStateChange: ((RideDisplayState) -> Void)?
    var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    var onReconnectScheduled: ((SessionConnectionRetry) -> Void)?
    var onCaptureEvent: ((CaptureEvent) -> Void)?
    var onScanStateChange: ((DevicePickerScanState) -> Void)?
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)?
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)?
    var onRideMapDecisionChange: ((MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void)?
    var onRideMapSnapshotChange: ((MobileRideMapSnapshotDto) -> Void)?
    var onRideMapErrorChange: ((MobileRideMapError) -> Void)?
    var onRideMapAvailabilityChange: ((MobileRideMapAvailability) -> Void)?
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?
    var onBluetoothRestorationResolved: ((String?) -> Void)?
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?
    private let scanState: DevicePickerScanState
    private let pairingSucceeds: Bool
    private let flushSucceeds: Bool
    private let restoredPlatformIdentifier: String?
    private let notifyBluetoothRestorationOnStart: Bool
    private(set) var pairedPlatformIdentifiers = [String]()
    private(set) var startCount = 0
    private(set) var probedPlatformIdentifiers = [String]()
    private(set) var recordedPlatformIdentifiers = [String]()
    private(set) var captureAnnotations = [String]()
    private(set) var flushCaptureCount = 0
    private(set) var disconnectCount = 0

    init(
        rows: [DevicePickerRow],
        pairingSucceeds: Bool = true,
        flushSucceeds: Bool = true,
        restoredPlatformIdentifier: String? = nil,
        notifyBluetoothRestorationOnStart: Bool = true,
        rideMapStorageError: String? = nil
    ) {
        scanState = DevicePickerScanState(status: .scanning, rows: rows)
        self.pairingSucceeds = pairingSucceeds
        self.flushSucceeds = flushSucceeds
        self.restoredPlatformIdentifier = restoredPlatformIdentifier
        self.notifyBluetoothRestorationOnStart = notifyBluetoothRestorationOnStart
        self.rideMapStorageError = rideMapStorageError
    }

    func start() {
        startCount += 1
        if notifyBluetoothRestorationOnStart {
            onBluetoothRestorationResolved?(restoredPlatformIdentifier)
        }
        onScanStateChange?(scanState)
    }

    func pair(platformIdentifier: String) -> Bool {
        pairedPlatformIdentifiers.append(platformIdentifier)
        return pairingSucceeds
    }

    func pair(platformIdentifier: String, model _: ElectricUnicycleModel) -> Bool {
        pair(platformIdentifier: platformIdentifier)
    }

    func probe(platformIdentifier: String) -> Bool {
        probedPlatformIdentifiers.append(platformIdentifier)
        return true
    }

    func recordOnly(platformIdentifier: String, note _: String?, annotations _: [String]) -> Bool {
        recordedPlatformIdentifiers.append(platformIdentifier)
        return true
    }
    func annotateCapture(label: String) {
        captureAnnotations.append(label)
    }
    func annotateCapture(key _: String, value _: String) {}
    func flushCapture() async -> Bool {
        flushCaptureCount += 1
        return flushSucceeds
    }

    func disconnectAndScan() {
        disconnectCount += 1
    }

    func now() -> MonotonicMilliseconds {
        MonotonicMilliseconds(0)
    }
}
