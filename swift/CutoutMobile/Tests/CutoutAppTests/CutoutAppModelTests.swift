import XCTest
@testable import CutoutApp
import CutoutMobile

final class CutoutAppModelTests: XCTestCase {
    private static let priorCaptureProgress = CaptureProgress(
        elapsedMilliseconds: 63_000,
        notificationCount: 42,
        fileSizeBytes: 12_288,
        queuedMessageCount: 0,
        writerError: nil
    )

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
    func testSupportedPickerActionStartsTheSelectedConnection() {
        let store = DevicePickerSelectionStore()
        store.clear()
        defer { store.clear() }
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
            let driver = SessionDriverSpy(rows: [row])
            let model = CutoutAppModel(core: driver, selectedDeviceStore: store)

            model.start()

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
    func testDisconnectKeepsSavedDeviceUntilExplicitForget() {
        let store = DevicePickerSelectionStore()
        store.save(platformIdentifier: "saved-device")
        defer { store.clear() }
        let model = CutoutAppModel()

        model.disconnectTransport()

        XCTAssertEqual(store.platformIdentifier, "saved-device")

        model.forgetSavedDevice()

        XCTAssertNil(store.platformIdentifier)
    }

    @MainActor
    func testConnectionFailureKeepsTheRememberedDevice() {
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
        store.clear()
        defer { store.clear() }
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
        XCTAssertEqual(model.connectionState.navigationIntent(isRecordOnlyCapture: false), .openRide(.vescOnewheel))

        driver.onPhaseChange?(.failed(.connectFailed("timed out")))
        XCTAssertEqual(model.connectionState.navigationIntent(isRecordOnlyCapture: false), .returnToPicker)
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
        defer { store.clear() }
        let model = CutoutAppModel(core: SessionDriverSpy(rows: []))

        XCTAssertTrue(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: "Unknown device"))

        XCTAssertEqual(store.platformIdentifier, "saved-device")
        XCTAssertTrue(model.hasSavedDevice)
        XCTAssertEqual(model.connectionState.navigationIntent(isRecordOnlyCapture: true), .stay)
    }

    @MainActor
    func testFinishCaptureFlushesOnceBeforeDisconnecting() {
        let driver = SessionDriverSpy(rows: [])
        let model = CutoutAppModel(core: driver)

        XCTAssertTrue(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: "Unknown device"))
        XCTAssertTrue(model.finishCapture())
        XCTAssertFalse(model.finishCapture())

        XCTAssertEqual(driver.flushCaptureCount, 1)
        XCTAssertEqual(driver.disconnectCount, 1)
    }

    @MainActor
    func testFinishCaptureKeepsTheCaptureRouteWhenWriterFlushFails() {
        let driver = SessionDriverSpy(rows: [], flushSucceeds: false)
        let model = CutoutAppModel(core: driver)

        XCTAssertTrue(model.recordOnly(platformIdentifier: "unknown-device", deviceKind: "Unknown device"))
        XCTAssertFalse(model.finishCapture())

        XCTAssertEqual(driver.flushCaptureCount, 1)
        XCTAssertEqual(driver.disconnectCount, 0)
        XCTAssertEqual(model.captureStatus, .failed)
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
        XCTAssertTrue(CaptureStatus.failed.isFailure)
        XCTAssertFalse(CaptureStatus.recordingLocally(fileName: "capture.jsonl").isFailure)
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

    func testCaptureSessionDetailsExposeTypedWriterHealth() {
        let healthyRows = captureSessionDetailRows(progress: CaptureProgress(
            elapsedMilliseconds: 63_000,
            notificationCount: 42,
            fileSizeBytes: 12_288,
            queuedMessageCount: 0,
            writerError: nil
        ))
        let failedRows = captureSessionDetailRows(progress: CaptureProgress(
            elapsedMilliseconds: 63_000,
            notificationCount: 42,
            fileSizeBytes: 12_288,
            queuedMessageCount: 0,
            writerError: "queue overrun"
        ))

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

        for _ in 0 ..< 10 {
            if model.liveActivityError != nil { break }
            await Task.yield()
        }

        XCTAssertEqual(model.liveActivityError, .authorizationDenied)

        await manager.setError(nil)
        XCTAssertTrue(model.pair(platformIdentifier: row.id))

        for _ in 0 ..< 10 {
            if await manager.startCount == 2 { break }
            await Task.yield()
        }

        let startCount = await manager.startCount
        XCTAssertEqual(startCount, 2)
        XCTAssertNil(model.liveActivityError)
    }

    func testExplicitUIFixtureOverridesPersistentFixture() {
        let fixture = CutoutUITestSessionFixture.resolve(
            persistedValue: "vesc-live-activity-auto",
            environmentValue: "vesc",
            arguments: ["--ui-test-euc"]
        )

        XCTAssertFalse(fixture?.startsLive ?? true)
        XCTAssertFalse(fixture?.isEuc ?? true)
    }
}

private actor FailingLiveActivityManager: LiveActivityRideLifecycleManaging {
    private var error: LiveActivityRideLifecycleError?
    private(set) var startCount = 0

    init(error: LiveActivityRideLifecycleError) {
        self.error = error
    }

    func start(snapshot _: LiveActivityRideSnapshot) async throws {
        startCount += 1
        if let error { throw error }
    }

    func update(snapshot _: LiveActivityRideSnapshot) async throws {}

    func end(reason _: LiveActivityRideLifecycleEndReason) async throws {}

    func setError(_ error: LiveActivityRideLifecycleError?) {
        self.error = error
    }
}

@MainActor
private final class SessionDriverSpy: CutoutSessionDriving {
    var onDisplayStateChange: ((RideDisplayState) -> Void)?
    var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    var onReconnectScheduled: ((SessionConnectionRetry) -> Void)?
    var onCaptureEvent: ((CaptureEvent) -> Void)?
    var onScanStateChange: ((DevicePickerScanState) -> Void)?
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)?
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto) -> Void)?
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?
    private let scanState: DevicePickerScanState
    private let pairingSucceeds: Bool
    private let flushSucceeds: Bool
    private(set) var pairedPlatformIdentifiers = [String]()
    private(set) var captureAnnotations = [String]()
    private(set) var flushCaptureCount = 0
    private(set) var disconnectCount = 0

    init(rows: [DevicePickerRow], pairingSucceeds: Bool = true, flushSucceeds: Bool = true) {
        scanState = DevicePickerScanState(status: .scanning, rows: rows)
        self.pairingSucceeds = pairingSucceeds
        self.flushSucceeds = flushSucceeds
    }

    func start() {
        onScanStateChange?(scanState)
    }

    func pair(platformIdentifier: String) -> Bool {
        pairedPlatformIdentifiers.append(platformIdentifier)
        return pairingSucceeds
    }

    func pair(platformIdentifier: String, model _: ElectricUnicycleModel) -> Bool {
        pair(platformIdentifier: platformIdentifier)
    }

    func recordOnly(platformIdentifier _: String, note _: String?, annotations _: [String]) -> Bool { true }
    func annotateCapture(label: String) {
        captureAnnotations.append(label)
    }
    func annotateCapture(key _: String, value _: String) {}
    func flushCapture() -> Bool {
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
