import CutoutMobile
import CutoutMobileFFI
import Foundation
import SwiftUI
import XCTest

@testable import CutoutApp

#if os(macOS)
    import AppKit
#endif

@MainActor
final class EucRideInstrumentTests: XCTestCase {
    func testTimerUpdatesCachedVehicleChoiceThroughRegistrarWithoutTelemetry() async throws {
        let suiteName = "EucRideInstrumentTests-device-type-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = DevicePickerSelectionStore(defaults: defaults)
        let identifier = "controller-\(UUID().uuidString)"
        let candidate = DevicePickerDiscoveryCandidate(
            platformIdentifier: identifier, displayName: "Controller", productCategory: "VESC",
            evidence: "fixture", detail: "fixture",
            support: .supported(connectionRoute: .vesc, electricUnicycleModel: nil),
            symbolName: "circle.hexagongrid.circle"
        )
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: candidate, telemetry: nil,
                protocolNotifications: [
                    Data([
                        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101,
                        114, 104, 97, 115, 104, 0, 38, 208, 3,
                    ])
                ], appliesProtocolNotificationSteps: true, connectionDelayMilliseconds: 0
            ), suppressNativeLocation: true, selectedDeviceStore: store
        )
        let manager = CutoutUITestSessionFixture.euc.makeLiveActivityManager()
        XCTAssertTrue(manager is CutoutUITestLiveActivityManager)
        let model = CutoutAppModel(core: core, selectedDeviceStore: store, liveActivityManager: manager)
        defer { core.disconnectAndScan() }
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: identifier))
        let deadline = ContinuousClock.now + .seconds(5)
        var sawPending = false
        while ContinuousClock.now < deadline {
            let requirement = model.device.deviceSessionSnapshot?.identity?.classificationRequirement
            if requirement == .pending { sawPending = true }
            if requirement == .userSelectionRequired { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let snapshot = try XCTUnwrap(model.device.deviceSessionSnapshot)
        let identity = try XCTUnwrap(snapshot.identity)
        XCTAssertTrue(sawPending)
        XCTAssertEqual(snapshot.connection.token?.platformIdentifier, identifier)
        XCTAssertEqual(identity.classificationRequirement, .userSelectionRequired)
        XCTAssertEqual(identity.vescApplication, .absent)
        XCTAssertEqual(identity.vehicleKind, .unknown)
        XCTAssertNil(identity.model)
        XCTAssertNil(model.displayState.telemetry?.speed)
        XCTAssertNil(model.displayState.telemetry?.voltage)
    }

    func testOrdinaryRideFixturesDoNotCreateSystemLiveActivities() {
        for fixture in [
            CutoutUITestSessionFixture.euc, .dynamicEuc, .staleEuc, .vesc, .unknownDevice, .bluetoothUnavailable,
        ] {
            XCTAssertTrue(fixture.makeLiveActivityManager() is CutoutUITestLiveActivityManager)
        }
        for fixture in [
            CutoutUITestSessionFixture.autoVescLiveActivity, .autoDynamicVescLiveActivity,
            .autoCriticalVescLiveActivity, .autoUnavailableVescLiveActivity, .autoStaleVescLiveActivity,
        ] {
            XCTAssertTrue(fixture.makeLiveActivityManager() is LiveActivityRideActivityKitManager)
        }
    }

    func testRefloatFixtureResolvesApplicationThroughSubmittedQueryWithoutUserClassification() async throws {
        let fixture = CutoutUITestSessionFixture.vesc
        let suiteName = "EucRideInstrumentTests-refloat-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = DevicePickerSelectionStore(defaults: defaults)
        let core = CutoutSessionCore(
            testScript: fixture.testScript, suppressNativeLocation: true, selectedDeviceStore: store
        )
        let model = CutoutAppModel(
            core: core, selectedDeviceStore: store, liveActivityManager: fixture.makeLiveActivityManager()
        )
        defer { core.disconnectAndScan() }
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: fixture.candidate.platformIdentifier))
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline {
            if model.device.deviceSessionSnapshot?.identity?.vescApplication == .refloat,
                model.displayState.telemetry?.voltage != nil
            {
                break
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        let identity = try XCTUnwrap(model.device.deviceSessionSnapshot?.identity)
        XCTAssertEqual(identity.protocol, .vesc)
        XCTAssertEqual(identity.vescApplication, .refloat)
        XCTAssertEqual(identity.vescApplicationQuery, .resolved)
        XCTAssertEqual(identity.classificationRequirement, .resolved)
        XCTAssertEqual(identity.classificationProvenance, .protocol)
        XCTAssertEqual(identity.vehicleKind, .onewheel)
        let ride = try XCTUnwrap(model.vescRideSnapshot)
        XCTAssertEqual(ride.application, .refloat)
        XCTAssertEqual(ride.vehicleKind, .onewheel)
    }

    func testEskateFixtureRequiresUserClassificationAndLeavesUnsupportedReadingsUnavailable() async throws {
        let fixture = CutoutUITestSessionFixture.eskate
        let suiteName = "EucRideInstrumentTests-eskate-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = DevicePickerSelectionStore(defaults: defaults)
        let core = CutoutSessionCore(
            testScript: fixture.testScript, suppressNativeLocation: true, selectedDeviceStore: store
        )
        let model = CutoutAppModel(
            core: core, selectedDeviceStore: store, liveActivityManager: fixture.makeLiveActivityManager()
        )
        defer { core.disconnectAndScan() }
        model.start()
        XCTAssertTrue(model.pair(platformIdentifier: fixture.candidate.platformIdentifier))
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline {
            if model.device.deviceSessionSnapshot?.identity?.vescApplication == .absent,
                model.displayState.telemetry?.voltage != nil
            {
                break
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        let identity = try XCTUnwrap(model.device.deviceSessionSnapshot?.identity)
        XCTAssertEqual(identity.protocol, .vesc)
        XCTAssertEqual(identity.vescApplication, .absent)
        XCTAssertEqual(identity.classificationRequirement, .userSelectionRequired)
        XCTAssertEqual(identity.vehicleKind, .unknown)
        XCTAssertTrue(fixture.makeLiveActivityManager() is CutoutUITestLiveActivityManager)
        let ride = try XCTUnwrap(model.vescRideSnapshot)
        XCTAssertEqual(ride.batteryVoltage, Voltage(value: 49_800))
        XCTAssertEqual(ride.motorCurrent, PhaseCurrent(value: 21_000))
        XCTAssertNil(ride.boardSpeed)
        XCTAssertNil(ride.batteryLevelReported)
        XCTAssertNil(ride.batteryLevelEstimated)
        XCTAssertNil(ride.footpad)
        XCTAssertNil(ride.dutyHeadroom)
        XCTAssertNil(ride.boardAngle)
    }

    func testIsolatedLiveActivityPreservesLifecycleReceiptsWithoutSystemEffects() async throws {
        let manager = CutoutUITestSessionFixture.euc.makeLiveActivityManager()
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: EucRideScreenState(phase: .live, displayState: RideDisplayState())
        )
        let identity = LiveActivityRideSessionIdentity(platformIdentifier: "fixture", sessionID: "ride")
        let started = try await manager.start(
            snapshot: snapshot, rideSessionIdentity: identity, staleAfterMilliseconds: 1_000,
            freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(started, .started(activityID: "ui-test-ride"))
        let adopted = try await manager.start(
            snapshot: snapshot, rideSessionIdentity: identity, staleAfterMilliseconds: 1_000,
            freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(adopted, .adopted(activityID: "ui-test-ride"))
        let updated = try await manager.update(
            snapshot: snapshot, staleAfterMilliseconds: 1_000, freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(updated.activityID, "ui-test-ride")
        let ended = try await manager.end(reason: .disconnected)
        XCTAssertEqual(ended.activityIDs, ["ui-test-ride"])
        let endedAgain = try await manager.end(reason: .disconnected)
        XCTAssertTrue(endedAgain.activityIDs.isEmpty)
        do {
            _ = try await manager.update(
                snapshot: snapshot, staleAfterMilliseconds: 1_000, freshnessExpiresAt: .distantFuture
            )
            XCTFail("An ended fixture must not acknowledge another activity update")
        } catch {
            XCTAssertEqual(error as? LiveActivityRideLifecycleError, .activityUnavailable)
        }
    }

    func testHeadroomClearsWhenTelemetryAgesWithoutAnotherSample() throws {
        let state = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000), operatingState: .riding, pwm: DutyCycle(permille: 680)
                )
            )
        )
        let fresh = view(state: state, now: 1_000)
        XCTAssertEqual(try XCTUnwrap(fresh.safetyBars.first?.progress), 0.32, accuracy: 0.001)
        XCTAssertEqual(fresh.safetyBars.first?.metricValue.displayText, "32%")
        XCTAssertTrue(view(state: state, now: 100_000).safetyBars.isEmpty)
        XCTAssertTrue(view(state: nil, now: 1_000).safetyBars.isEmpty)
    }

    func testDisconnectedTelemetryDoesNotFillTheGauge() {
        let state = EucRideScreenState(
            phase: .scanning,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000), operatingState: .riding, pwm: DutyCycle(permille: 680)
                )
            )
        )
        XCTAssertTrue(view(state: state, now: 1_000).safetyBars.isEmpty)
    }

    func testUnavailableGaugeNeverFillsAndZeroRemainsAvailable() {
        for value in [PevDashboardMetricValue.unavailable, .status(display: "N/A", accessibility: "Not applicable")] {
            XCTAssertNil(gauge(value: value, progress: 0.5).displayedProgress)
        }
        XCTAssertEqual(gauge(value: .available(display: "0%", accessibility: "0%"), progress: 0).displayedProgress, 0)
        XCTAssertEqual(
            gauge(value: .available(display: "100%", accessibility: "100%"), progress: 1).displayedProgress, 1)
    }

    #if os(macOS)
        func testBatteryStripKeepsItsSizeWhenTelemetryBecomesUnavailable() {
            func size(progress: Double?) -> CGSize {
                let host = NSHostingView(
                    rootView: PevInstrumentReading(
                        label: "Battery", value: .available(display: "78", accessibility: "78"),
                        unit: "%", pointSize: 52, identifier: "battery", showsProgress: true, progress: progress
                    ).environment(\.dynamicTypeSize, .large).frame(width: 180))
                return host.fittingSize
            }
            XCTAssertEqual(size(progress: 0.78), size(progress: nil))
        }
    #endif

    private func gauge(value: PevDashboardMetricValue, progress: Double) -> EucHeadroomGauge {
        EucHeadroomGauge(
            headroom: PevSafetyBar(
                id: .pwmHeadroom, label: "PWM headroom", metricValue: value, progress: progress, accent: .yellow)
        )
    }

    private func view(state: EucRideScreenState?, now: UInt64) -> EucRideScreenView {
        EucRideScreenView(
            rideState: state, rideTitle: "NOSFET Aero", now: MonotonicMilliseconds(now),
            captureStatusText: nil, connectionStatusText: "Connected",
            phoneLocationReadback: PhoneLocationReadback(
                snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
            )
        )
    }
}
