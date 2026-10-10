import CoreLocation
import CutoutMobileFFI
import Foundation
import SQLite3
import Synchronization
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

final class LockedRideMainThreadTests: XCTestCase {
    @MainActor
    func testLivePhasePresentationDoesNotWaitForHeldBleOwner() async throws {
        let suiteName = "LockedRideMainThreadTests.livePhase.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let script = CutoutUITestSessionFixture.euc.testScript
        let bleQueue = DispatchQueue(label: "live-phase-presentation.ble")
        let held = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        defer { release.signal() }
        let released = Mutex(false)
        let core = CutoutSessionCore(
            clock: MonotonicClock(),
            testScript: CutoutSessionTestScript(
                candidate: script.candidate, telemetry: script.telemetry,
                protocolNotifications: script.protocolNotifications,
                connectionDelayMilliseconds: 0),
            bleQueue: bleQueue)
        let model = CutoutAppModel(core: core, selectedDeviceStore: DevicePickerSelectionStore(defaults: defaults))
        let handler = core.onPhaseChange
        defer { core.onPhaseChange = handler }
        let live = expectation(description: "The app receives live while the BLE owner remains held")
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            guard phase == .live else {
                handler?(presentation)
                return
            }
            bleQueue.async {
                held.signal()
                // The old synchronous getter must fail without hanging XCTest.
                _ = release.wait(timeout: .now() + .seconds(1))
                released.withLock { $0 = true }
            }
            XCTAssertEqual(held.wait(timeout: .now() + .seconds(1)), .success)
            handler?(presentation)
            XCTAssertFalse(
                released.withLock { $0 },
                "Phase presentation must use captured facts instead of entering the held BLE owner")
            XCTAssertEqual(model.phase, .live)
            XCTAssertEqual(model.selectedRideIdentifier, script.candidate.platformIdentifier)
            XCTAssertEqual(model.selectedConnectionRoute, .electricUnicycle)
            XCTAssertEqual(core.connectionSnapshot.readiness, .verified)
            release.signal()
            live.fulfill()
        }
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: script.candidate.platformIdentifier))
        await fulfillment(of: [live], timeout: 3)
        core.disconnectAndScan()
    }

    func testDefaultReconnectTimerLeavesMainResponsiveWhileBleOwnerIsHeld() async {
        let now = Mutex<UInt64>(1_000)
        let released = Mutex(false)
        let release = DispatchSemaphore(value: 0)
        defer { release.signal() }
        let bleQueue = DispatchQueue(label: "reconnect-main-responsiveness.ble")
        let queueKey = DispatchSpecificKey<Bool>()
        bleQueue.setSpecific(key: queueKey, value: true)
        let original = CutoutUITestSessionFixture.euc.testScript
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(now.withLock { $0 }) },
            testScript: CutoutSessionTestScript(
                candidate: original.candidate, telemetry: nil, connectionDelayMilliseconds: 60_000),
            reconnectJitter: { 0 },
            bleQueue: bleQueue
        )
        let identity = original.candidate.platformIdentifier
        _ = core.rideSessionStateHandle.beginConnectionAttempt(platformIdentifier: identity, nowMs: 0)
        let originalToken = core.connectionSnapshot.token
        XCTAssertNotNil(originalToken)
        let scheduled = expectation(description: "Rust schedules the first retry at its deadline")
        let held = expectation(description: "The existing BLE owner is held across the timer deadline")
        let mainProgress = expectation(description: "Main progresses before the BLE owner is released")
        let retried = expectation(description: "The admitted retry executes on the BLE owner")
        let reconnectCount = Mutex(0)
        core.onReconnectScheduled = { snapshot in
            XCTAssertEqual(snapshot.attempt, 1)
            XCTAssertEqual(snapshot.deadline.rawValue, 1_200)
            scheduled.fulfill()
        }
        core.handleTransportTermination(platformIdentifier: identity, error: nil) {
            XCTAssertFalse(Thread.isMainThread)
            XCTAssertEqual(DispatchQueue.getSpecific(key: queueKey), true)
            XCTAssertTrue(released.withLock { $0 })
            XCTAssertEqual(core.connectionSnapshot.token?.platformIdentifier, identity)
            XCTAssertNotEqual(core.connectionSnapshot.token, originalToken)
            reconnectCount.withLock { $0 += 1 }
            retried.fulfill()
        }
        now.withLock { $0 = 1_200 }
        bleQueue.async {
            held.fulfill()
            // A bounded fallback lets the old Main-to-BLE wait fail instead of hanging XCTest.
            _ = release.wait(timeout: .now() + .seconds(1))
            released.withLock { $0 = true }
        }
        await fulfillment(of: [held, scheduled], timeout: 2)
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(300)) {
            XCTAssertFalse(
                released.withLock { $0 },
                "The default reconnect timer must not make Main wait for the BLE owner")
            mainProgress.fulfill()
        }
        await fulfillment(of: [mainProgress], timeout: 2)
        release.signal()
        await fulfillment(of: [retried], timeout: 2)
        XCTAssertEqual(reconnectCount.withLock { $0 }, 1)
        core.disconnectAndScan()
    }

    @MainActor
    func testScriptedConnectionCompletionLeavesMainResponsiveDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let original = CutoutUITestSessionFixture.euc.testScript
        let clock = MonotonicClock()
        let recordingErrors = Mutex<[MobileRideMapError]>([])
        let recorder = CutoutSessionRideMapRecorder(
            state: fixture.state,
            clock: clock,
            wallClock: { Date() },
            publishSnapshot: { _ in },
            publishDecisions: { _ in },
            publishError: { error, _ in recordingErrors.withLock { $0.append(error) } },
            publishAvailability: { _, _ in },
            recordDiagnostic: { _ in },
            onLocationDemandChanged: {}
        )
        let core = CutoutSessionCore(
            clock: clock,
            testScript: CutoutSessionTestScript(
                candidate: original.candidate,
                telemetry: original.telemetry,
                protocolNotifications: original.protocolNotifications,
                connectionDelayMilliseconds: 0
            ),
            rideMapState: fixture.state,
            rideMapRecorder: recorder
        )
        let mainProgress = expectation(
            description: "Main processes presentation while the protocol owner awaits SQLite")
        let live = expectation(description: "Actual protocol evidence reaches live after storage is released")
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            if phase == .subscribing {
                // This deadline follows the scripted zero-delay completion. The oracle
                // checks the actual SQLite lock, rather than elapsed presentation time.
                DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(10)) {
                    XCTAssertFalse(
                        fixture.unlocked.withLock { $0 },
                        "Delayed scripted protocol setup must not make Main wait for the voltage-sag SQLite query"
                    )
                    mainProgress.fulfill()
                }
            }
            if phase == .live { live.fulfill() }
        }
        core.start()
        try await fixture.holdWrite()
        XCTAssertTrue(core.pair(platformIdentifier: original.candidate.platformIdentifier))
        await fulfillment(of: [mainProgress], timeout: 2)
        fixture.release.signal()
        await fulfillment(of: [live], timeout: 2)
        XCTAssertEqual(core.connectionSnapshot.token?.platformIdentifier, original.candidate.platformIdentifier)
        XCTAssertEqual(core.connectionSnapshot.readiness, .verified)
        core.disconnectAndScan()
        // Live transport publication precedes Rust's asynchronous ride admission receipt.
        // Settle the actual recording owner before the fixture's synchronous Stop.
        try await recorder.checkpoint()
        XCTAssertEqual(recordingErrors.withLock { $0 }, [])
        try await fixture.finish()
    }

    @MainActor
    func testSelectedDeviceConstructionLeavesMainResponsiveDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let identifier = "selection-construction-\(UUID().uuidString)"
        let previous = try await Task.detached {
            let previous = try fixture.database.selectedDevice()
            try fixture.database.rememberSelectedDevice(
                platformIdentifier: identifier, displayName: "Saved wheel", updatedAtMilliseconds: 1_000)
            return previous
        }.value
        let store = DevicePickerSelectionStore(database: fixture.database)
        try await fixture.holdWrite()

        let model = DevicePresentationModel(selectedDeviceStore: store)
        let restoration = model.restoreSavedSelection()

        XCTAssertFalse(
            fixture.unlocked.withLock { $0 },
            "Constructing native presentation must not wait for the selected-device SQLite query")
        fixture.release.signal()
        await restoration?.value
        XCTAssertEqual(model.rideMapVehicleIdentity, identifier)
        XCTAssertEqual(model.rideMapVehicleName, "Saved wheel")
        try await fixture.finish()
        try await Self.restoreSelectedDevice(previous, database: fixture.database)
        _ = model
    }

    @MainActor
    func testSelectedDeviceAndNamePresentationLeavesMainResponsiveDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let identifier = "selection-presentation-\(UUID().uuidString)"
        let previous = try await Task.detached {
            let previous = try fixture.database.selectedDevice()
            try fixture.database.rememberSelectedDevice(
                platformIdentifier: identifier, displayName: "Saved wheel", updatedAtMilliseconds: 1_000)
            return previous
        }.value
        let store = DevicePickerSelectionStore(database: fixture.database)
        let restoredSelection = await store.load()
        let model = DevicePresentationModel(selectedDeviceStore: store, restoredSelection: restoredSelection)
        try await fixture.holdWrite()

        for _ in 0..<3 {
            XCTAssertEqual(model.rideMapVehicleIdentity, identifier)
            XCTAssertEqual(model.rideMapVehicleName, "Saved wheel")
            XCTAssertEqual(model.persistedVehicleName(for: identifier), "Saved wheel")
            XCTAssertNil(model.rideMapVehicleName(for: "unnamed-history-wheel"))
        }

        XCTAssertFalse(
            fixture.unlocked.withLock { $0 },
            "Repeated selection/name presentation must not query or migrate SQLite on Main")
        fixture.release.signal()
        try await fixture.finish()
        try await Self.restoreSelectedDevice(previous, database: fixture.database)
    }

    private static func restoreSelectedDevice(_ identifier: String?, database: RideDatabaseHandle) async throws {
        try await Task.detached {
            if let identifier {
                try database.rememberSelectedDevice(
                    platformIdentifier: identifier, displayName: nil, updatedAtMilliseconds: 2_000)
            } else {
                try database.clearSelectedDevice()
            }
        }.value
    }

    @MainActor
    func testProtocolIdentityNamePersistenceLeavesMainResponsiveDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let identifier = "protocol-name-\(UUID().uuidString)"
        let previous = try await Task.detached { try fixture.database.selectedDevice() }.value
        let store = DevicePickerSelectionStore(database: fixture.database)
        let model = DevicePresentationModel(selectedDeviceStore: store)
        let candidate = DevicePickerDiscoveryCandidate(
            platformIdentifier: identifier, displayName: "Protocol wheel", productCategory: "Electric unicycle",
            evidence: "protocol reply", detail: "resolved device",
            support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
            symbolName: "circle.hexagongrid.circle")
        try await fixture.holdWrite()

        model.applyProtocolIdentityCandidate(candidate, allowsRidePresentation: true)

        XCTAssertFalse(
            fixture.unlocked.withLock { $0 },
            "An automatic protocol identity callback must not wait for device-name lookup or persistence")
        XCTAssertEqual(model.selectedRideIdentifier, identifier)
        XCTAssertEqual(model.rideMapVehicleName, "Protocol wheel")
        fixture.release.signal()
        await model.waitForProtocolNamePersistence()
        let durable = try await Task.detached {
            (try fixture.database.selectedDevice(), try fixture.database.deviceName(platformIdentifier: identifier))
        }.value
        XCTAssertEqual(durable.0, previous, "Automatic naming must not replace the saved selection")
        XCTAssertEqual(durable.1, "Protocol wheel")
        try await fixture.finish()
        try await Self.restoreSelectedDevice(previous, database: fixture.database)
    }

    @MainActor
    func testSameIdentityProtocolNamesPersistLatestValueWithoutReselectingDevice() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let previous = try await Task.detached { try fixture.database.selectedDevice() }.value
        let firstIdentity = "protocol-first-\(UUID().uuidString)"
        let model = DevicePresentationModel(selectedDeviceStore: DevicePickerSelectionStore(database: fixture.database))
        func candidate(_ identity: String, _ name: String) -> DevicePickerDiscoveryCandidate {
            DevicePickerDiscoveryCandidate(
                platformIdentifier: identity, displayName: name, productCategory: "Electric unicycle",
                evidence: "protocol reply", detail: "resolved device",
                support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
                symbolName: "circle.hexagongrid.circle")
        }
        try await fixture.holdWrite()
        model.applyProtocolIdentityCandidate(candidate(firstIdentity, "First wheel"), allowsRidePresentation: true)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        model.applyProtocolIdentityCandidate(
            candidate(firstIdentity, "Old pending name"), allowsRidePresentation: true)
        model.applyProtocolIdentityCandidate(
            candidate(firstIdentity, "Latest pending name"), allowsRidePresentation: true)
        XCTAssertEqual(model.selectedRideIdentifier, firstIdentity)
        fixture.release.signal()
        await model.waitForProtocolNamePersistence()
        let durable = try await Task.detached {
            (
                try fixture.database.selectedDevice(),
                try fixture.database.deviceName(platformIdentifier: firstIdentity)
            )
        }.value
        XCTAssertEqual(durable.0, previous)
        XCTAssertEqual(durable.1, "Latest pending name")
        try await fixture.finish()
    }

    @MainActor
    func testForgetRetiresHeldSelectionRestoreAndAutomaticNameCannotReselect() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let identity = "selection-forget-\(UUID().uuidString)"
        let previous = try await Task.detached {
            let previous = try fixture.database.selectedDevice()
            try fixture.database.rememberSelectedDevice(
                platformIdentifier: identity, displayName: "Saved wheel", updatedAtMilliseconds: 1_000)
            return previous
        }.value
        let model = DevicePresentationModel(selectedDeviceStore: DevicePickerSelectionStore(database: fixture.database))
        try await fixture.holdWrite()
        var restored = false
        let restoration = model.restoreSavedSelection { restored = true }
        model.applyProtocolIdentityCandidate(
            DevicePickerDiscoveryCandidate(
                platformIdentifier: identity, displayName: "Protocol wheel", productCategory: "Electric unicycle",
                evidence: "protocol reply", detail: "resolved device",
                support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
                symbolName: "circle.hexagongrid.circle"),
            allowsRidePresentation: true)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        model.applyProtocolIdentityCandidate(
            DevicePickerDiscoveryCandidate(
                platformIdentifier: identity, displayName: "Latest protocol wheel",
                productCategory: "Electric unicycle",
                evidence: "protocol reply", detail: "resolved device",
                support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
                symbolName: "circle.hexagongrid.circle"),
            allowsRidePresentation: true)
        fixture.release.signal()
        model.forgetSavedDevice()
        await restoration?.value
        await model.waitForProtocolNamePersistence()
        XCTAssertFalse(restored, "An old selection read must not invoke restoration after explicit forget")
        XCTAssertNil(model.savedPlatformIdentifier)
        XCTAssertFalse(model.hasSavedDevice)
        let durable = try await Task.detached {
            (try fixture.database.selectedDevice(), try fixture.database.deviceName(platformIdentifier: identity))
        }.value
        XCTAssertNil(durable.0, "A delayed automatic name write must not resurrect selection")
        XCTAssertEqual(durable.1, "Latest protocol wheel", "Explicit forget must settle the latest pending old name")
        try await fixture.finish()
        try await Self.restoreSelectedDevice(previous, database: fixture.database)
    }

    @MainActor
    func testLatestBluetoothNameRestorationSurvivesAnOlderHeldQuery() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let first = "name-restore-first-\(UUID().uuidString)"
        let latest = "name-restore-latest-\(UUID().uuidString)"
        try await Task.detached {
            try fixture.database.saveDeviceName(
                platformIdentifier: first, displayName: "First wheel", updatedAtMilliseconds: 1_000)
            try fixture.database.saveDeviceName(
                platformIdentifier: latest, displayName: "Latest wheel", updatedAtMilliseconds: 1_000)
        }.value
        let model = DevicePresentationModel(selectedDeviceStore: DevicePickerSelectionStore(database: fixture.database))
        try await fixture.holdWrite()
        model.restoreVehicleName(for: first)
        try await Task.sleep(for: .milliseconds(50))
        model.connectionState = .identified(
            ConnectionSelection(
                platformIdentifier: latest, title: localizedAppText("setup.device"), route: .electricUnicycle))
        model.restoreVehicleName(for: latest)
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        fixture.release.signal()
        await model.waitForVehicleNameRestoration()
        XCTAssertEqual(model.persistedVehicleName(for: first), "First wheel")
        XCTAssertEqual(model.persistedVehicleName(for: latest), "Latest wheel")
        XCTAssertEqual(model.selectedRideIdentifier, latest)
        XCTAssertEqual(
            model.selectedRideTitle, "Latest wheel", "An older name result cannot overwrite current identity")
        try await fixture.finish()
    }

    @MainActor
    func testAutomaticPairDoesNotWaitForHeldProtocolNamePersistence() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let identity = "automatic-pair-\(UUID().uuidString)"
        let store = DevicePickerSelectionStore(database: fixture.database)
        let previous = try await Task.detached {
            let previous = try fixture.database.selectedDevice()
            try fixture.database.rememberSelectedDevice(
                platformIdentifier: identity, displayName: "Saved wheel", updatedAtMilliseconds: 1_000)
            return previous
        }.value
        let snapshot = await store.load()
        let model = DevicePresentationModel(selectedDeviceStore: store, restoredSelection: snapshot)
        model.scanState = DevicePickerScanState(
            status: .scanning,
            rows: [
                DevicePickerRow(
                    id: identity, title: "Advertisement name", subtitle: "Electric unicycle",
                    detail: "Supported device",
                    state: .supported(action: "Connect"), symbolName: "circle.hexagongrid.circle",
                    connectionRoute: .electricUnicycle)
            ])
        try await fixture.holdWrite()
        model.applyProtocolIdentityCandidate(
            DevicePickerDiscoveryCandidate(
                platformIdentifier: identity, displayName: "Protocol wheel", productCategory: "Electric unicycle",
                evidence: "protocol reply", detail: "resolved device",
                support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
                symbolName: "circle.hexagongrid.circle"),
            allowsRidePresentation: true)
        try await Task.sleep(for: .milliseconds(50))
        let outcome = model.pair(
            platformIdentifier: identity, mayRetryCurrentSelection: true, persistAcceptedSelection: false
        ) { _ in true }
        guard case .accepted = outcome else { return XCTFail("Expected accepted automatic pair") }
        XCTAssertFalse(fixture.unlocked.withLock { $0 }, "Passive autopair must not join the name persistence receipt")
        XCTAssertEqual(model.savedPlatformIdentifier, identity)
        fixture.release.signal()
        await model.waitForProtocolNamePersistence()
        try await fixture.finish()
        try await Self.restoreSelectedDevice(previous, database: fixture.database)
    }

    @MainActor
    func testAsyncMusicAdmissionsLeaveMainResponsiveDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        try await fixture.holdWrite()
        let currentRead = Task { @MainActor in try await fixture.state.currentMusicHistoryAsync() }
        let storedRead = Task { @MainActor in
            try await fixture.state.storedMusicHistoryAsync(rideID: fixture.rideID)
        }
        let policyWrite = Task { @MainActor in try await fixture.state.setMusicHistoryPolicyAsync(.opaqueItem) }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertFalse(
            fixture.unlocked.withLock { $0 },
            "Nonisolated async history admission and receipt polling must leave Main responsive")
        fixture.release.signal()
        let current = try await currentRead.value
        let stored = try await storedRead.value
        try await policyWrite.value
        XCTAssertNotNil(current)
        XCTAssertTrue(current?.events.isEmpty == true)
        XCTAssertTrue(stored.events.isEmpty)
        let policy = await fixture.state.currentMusicHistoryPolicyAsync()
        XCTAssertEqual(policy, .opaqueItem)
        try await fixture.finish()
    }

    @MainActor
    func testAvailabilityQueryDoesNotRetainCoreDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        var core: CutoutSessionCore? = CutoutSessionCore(
            clock: MonotonicClock(), rideMapState: fixture.state,
            phoneLocationAdapter: AvailablePhoneLocationAdapter())
        weak let releasedCore = core
        try await fixture.holdWrite()
        core?.publishRideMapAvailabilityOnMain()
        try await Task.sleep(for: .milliseconds(50))
        core = nil
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        XCTAssertNil(releasedCore, "A queued environment query must not retain Core or its location owner")
        try await fixture.finish()
    }

    @MainActor
    func testDurationQueryDoesNotRetainLiveRideDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        var model: LiveRideModel? = LiveRideModel(
            state: fixture.state, storageError: nil, availability: .ready, now: { 2_000 })
        weak let releasedModel = model
        try await fixture.holdWrite()
        model?.setSceneActive(true)
        let query = model?.refreshDuration()
        try await Task.sleep(for: .milliseconds(50))
        model = nil
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        XCTAssertNil(releasedModel, "A queued duration query must not retain live presentation")
        fixture.release.signal()
        await query?.value
        try await fixture.finish()
    }

    @MainActor
    func testErrorQueryDoesNotRetainLiveRideDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let current = await fixture.state.currentSnapshotAsync()
        let snapshot = try XCTUnwrap(current)
        var model: LiveRideModel? = LiveRideModel(
            state: fixture.state, storageError: nil, availability: .ready, now: { 2_000 })
        weak let releasedModel = model
        try await fixture.holdWrite()
        let query = model?.applyError(
            MobileRideMapErrorEvent(
                context: MobileRideMapErrorContext(snapshot: snapshot), error: .storageError("retained failure")))
        try await Task.sleep(for: .milliseconds(50))
        model = nil
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        XCTAssertNil(releasedModel, "A queued authoritative error query must not retain live presentation")
        fixture.release.signal()
        await query?.value
        try await fixture.finish()
    }

    @MainActor
    func testLiveRideRestoreDoesNotWaitForTheSharedRustMutexDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let model = LiveRideModel(state: fixture.state, storageError: nil, availability: .ready, now: { 2_000 })
        try await fixture.holdWrite()
        let restore = model.restore()
        XCTAssertFalse(fixture.unlocked.withLock { $0 }, "Main must return before SQLite unlocks")
        fixture.release.signal()
        await restore?.value
        XCTAssertEqual(model.snapshot?.rideID, fixture.rideID)
        try await fixture.finish()
    }

    @MainActor
    func testAvailabilityMutationDoesNotWaitForTheSharedRustMutexDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let acquisition = expectation(description: "Authoritative acquisition delivered after SQLite unlock")
        let phone = AvailablePhoneLocationAdapter()
        phone.onDemand = { demand in
            if demand == .record { acquisition.fulfill() }
        }
        let core = CutoutSessionCore(clock: MonotonicClock(), rideMapState: fixture.state, phoneLocationAdapter: phone)
        try await fixture.holdWrite()
        core.publishRideMapAvailabilityOnMain()
        XCTAssertFalse(fixture.unlocked.withLock { $0 }, "Availability must enqueue off Main before SQLite unlocks")
        fixture.release.signal()
        await fulfillment(of: [acquisition], timeout: 2)
        try await fixture.finish()
    }
}

/// The second connection and its transaction belong exclusively to this fixture's queue.
/// Rust owns the real write and holds the same mutex used by GPS checkpoint settlement.
final class SQLiteRideStall: @unchecked Sendable {
    let state: MobileRideMapState
    let rideID: String
    let release = DispatchSemaphore(value: 0)
    let unlocked = Mutex(false)
    let database: RideDatabaseHandle
    private let path: String
    private let previousAutostart: Bool
    private let entered = Mutex(false)
    private let writeFinished = Mutex(false)
    private var write: Task<Void, Error>?

    private init(
        database: RideDatabaseHandle, state: MobileRideMapState, rideID: String, path: String, previousAutostart: Bool
    ) {
        self.database = database
        self.state = state
        self.rideID = rideID
        self.path = path
        self.previousAutostart = previousAutostart
    }

    static func make() async throws -> SQLiteRideStall {
        try await Task.detached {
            let state = MobileRideMapState()
            let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
            let previousAutostart = try state.rideAutostartEnabled()
            try state.setRideAutostartEnabled(false)
            let snapshot = try state.startGpsOnly(atMs: 1_000)
            return SQLiteRideStall(
                database: database, state: state, rideID: snapshot.rideID, path: MobileRideMapState.debugDatabasePath,
                previousAutostart: previousAutostart)
        }.value
    }

    func holdWrite() async throws {
        DispatchQueue(label: "test.cutout.sqlite-stall").async { [self] in
            var connection: OpaquePointer?
            XCTAssertEqual(sqlite3_open(path, &connection), SQLITE_OK)
            defer { sqlite3_close(connection) }
            XCTAssertEqual(sqlite3_exec(connection, "BEGIN IMMEDIATE", nil, nil, nil), SQLITE_OK)
            entered.withLock { $0 = true }
            // The old Main-side call blocks; the fail-safe release gives RED an assertion
            // failure instead of hanging the supported test runner.
            _ = release.wait(timeout: .now() + .seconds(1))
            unlocked.withLock { $0 = true }
            XCTAssertEqual(sqlite3_exec(connection, "COMMIT", nil, nil, nil), SQLITE_OK)
        }
        while !entered.withLock({ $0 }) { try await Task.sleep(for: .milliseconds(1)) }
        write = Task.detached { [self] in
            try state.setRideAutostartEnabled(true)
            writeFinished.withLock { $0 = true }
        }
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertFalse(writeFinished.withLock { $0 }, "The production write must remain blocked by real SQLite")
        XCTAssertFalse(unlocked.withLock { $0 })
    }

    func finish() async throws {
        release.signal()
        try await write?.value
        if let snapshot = await state.currentSnapshotAsync(), snapshot.state.isOpen {
            let now = UInt64(ProcessInfo.processInfo.systemUptime * 1_000)
            _ = try await state.performLifecycleCommand(
                event: .stop, expected: XCTUnwrap(snapshot.commandToken), atMs: max(3_000, now))
        }
        try await Task.detached { [self] in
            if state.currentSnapshot()?.state == .stopped { _ = try state.discard() }
            try state.setRideAutostartEnabled(previousAutostart)
        }.value
    }
}

@MainActor
private final class AvailablePhoneLocationAdapter: CutoutSessionPhoneLocationAdapting {
    var latestSample: MobilePhoneLocationSampleDto? { nil }
    var authorizationStatus: CLAuthorizationStatus { .authorizedAlways }
    var servicesEnabled: Bool? { true }
    func start() {}
    func clear() {}
    var onDemand: ((MobileRideMapLocationDemandDto) -> Void)?
    func updateDemand(_ demand: MobileRideMapLocationDemandDto) { onDemand?(demand) }
    func locationManagerDidChangeAuthorization(_: CLLocationManager) {}
    func locationManager(_: CLLocationManager, didUpdateLocations _: [CLLocation]) {}
}
