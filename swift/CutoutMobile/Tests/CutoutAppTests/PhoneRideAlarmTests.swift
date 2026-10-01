import CutoutMobile
import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp

final class PhoneRideAlarmTests: XCTestCase {
    func testAuthorizationCapabilityKeepsProvisionalDeliveryQuiet() {
        XCTAssertEqual(
            PhoneRideAlarmAuthorization.permitted(
                alerts: true,
                sounds: true,
                quietly: true
            ).capability,
            MobilePhoneAlarmDeliveryCapabilityDto(
                canSchedule: true,
                playsSound: false
            )
        )
        XCTAssertEqual(
            PhoneRideAlarmAuthorization.permitted(
                alerts: true,
                sounds: true,
                quietly: false
            ).capability,
            MobilePhoneAlarmDeliveryCapabilityDto(
                canSchedule: true,
                playsSound: true
            )
        )
    }

    func testAlarmRenderingNamesPwmDutyAndComplementaryHeadroom() {
        let alert = PhoneRideAlarmAlert(
            event: .pwmDuty(dutyPercent: 80, headroomPercent: 20)
        )

        XCTAssertEqual(alert.title, "Phone alarm: PWM duty")
        XCTAssertEqual(alert.body, "80% PWM duty · 20% headroom remaining")
    }

    func testControllerAlarmRenderingReusesRideAccessibilityText() {
        let alert = PhoneRideAlarmAlert(
            event: .controllerWarning(warning: .motorTemperature)
        )

        XCTAssertEqual(alert.title, "Phone alarm: Controller warning")
        XCTAssertEqual(alert.body, "Warning. Motor temperature is high. Stop safely and let it cool.")
    }

    @MainActor
    func testPhoneAlarmErrorsUseLocalizedMessages() {
        let expected: [(MobilePhoneAlarmError, String)] = [
            (.NoActiveDevice, "No wheel is selected."),
            (.InvalidDeviceIdentity, "The selected wheel identity is invalid."),
            (.InvalidPwmDutyThreshold, "PWM duty must be between 1% and 100%."),
            (.InvalidPwmHeadroomThreshold, "PWM headroom must be between 0% and 99%."),
            (.TooManyDevices, "Too many phone alarm settings are saved."),
            (.DeviceIdentityChanged, "The selected wheel changed. Try again."),
            (.StorageFailure, "Phone alarm settings could not be saved."),
        ]

        for (error, message) in expected {
            XCTAssertEqual(CutoutAppModel.phoneAlarmErrorText(error), message)
        }
    }

    @MainActor
    func testPermissionCompletionCannotEnableAReplacementDevice() async throws {
        let fixture = CutoutUITestSessionFixture.autoCriticalVescLiveActivity
        let firstIdentity = fixture.candidate.platformIdentifier
        let secondIdentity = "replacement-wheel"
        let core = CutoutSessionCore(testScript: fixture.testScript)
        _ = try core.rideSessionStateHandle.activatePhoneAlarmDevice(
            deviceIdentity: firstIdentity
        )
        let delivery = SuspendedAuthorizationPhoneRideAlarmDeliverySpy()
        let model = CutoutAppModel(core: core, phoneAlarmDelivery: delivery)

        let enabling = Task {
            await model.setPhoneAlarmsEnabled(
                true,
                deviceIdentity: firstIdentity
            )
        }
        for _ in 0..<1_000 where !delivery.hasPendingAuthorizationRequest {
            await Task.yield()
        }
        XCTAssertTrue(delivery.hasPendingAuthorizationRequest)

        _ = try core.rideSessionStateHandle.activatePhoneAlarmDevice(
            deviceIdentity: secondIdentity
        )
        delivery.resolveAuthorization(.permitted(alerts: true, sounds: true, quietly: false))
        await enabling.value

        XCTAssertEqual(model.phoneAlarmSettings?.deviceIdentity, secondIdentity)
        XCTAssertEqual(core.rideSessionStateHandle.phoneAlarmPreferences()?.enabled, false)
    }

    @MainActor
    func testCancelledAuthorizationRefreshCannotOverwriteNewerCapability() async {
        let delivery = RefreshRacePhoneRideAlarmDeliverySpy()
        let model = CutoutAppModel(
            core: CutoutSessionCore(
                testScript: CutoutUITestSessionFixture.autoCriticalVescLiveActivity.testScript
            ),
            phoneAlarmDelivery: delivery
        )
        for _ in 0..<1_000 where !delivery.hasSuspendedRefresh {
            await Task.yield()
        }
        XCTAssertTrue(delivery.hasSuspendedRefresh)

        model.refreshPhoneAlarmAuthorization()
        let expected = PhoneRideAlarmAuthorization.permitted(
            alerts: true,
            sounds: true,
            quietly: false
        )
        for _ in 0..<1_000 where model.phoneAlarmAuthorization != expected {
            await Task.yield()
        }
        XCTAssertEqual(model.phoneAlarmAuthorization, expected)

        delivery.resolveSuspendedRefresh(.notDetermined)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(model.phoneAlarmAuthorization, expected)
    }

    @MainActor
    func testInvalidatedSuccessfulNativeDeliveryIsCancelled() async throws {
        let fixture = CutoutUITestSessionFixture.autoCriticalVescLiveActivity
        let core = CutoutSessionCore(testScript: fixture.testScript)
        let delivery = SuspendedPhoneRideAlarmDeliverySpy()
        let model = CutoutAppModel(core: core, phoneAlarmDelivery: delivery)
        let request = MobilePhoneAlarmDeliveryRequestDto(
            id: 42,
            event: .pwmDuty(dutyPercent: 80, headroomPercent: 20),
            playsSound: true
        )
        model.applyPhoneAlarmActions(
            MobilePhoneAlarmActionsDto(
                schedule: [request],
                cancelRequestIds: []
            ))
        for _ in 0..<1_000 where delivery.pendingRequests.isEmpty {
            await Task.yield()
        }
        XCTAssertEqual(delivery.pendingRequests.first?.id, request.id)
        model.applyPhoneAlarmActions(
            MobilePhoneAlarmActionsDto(
                schedule: [],
                cancelRequestIds: [request.id]
            ))
        delivery.resolveAllDeliveries()
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(delivery.cancelledRequestIDs, [request.id, request.id])
    }
}

@MainActor
private final class SuspendedAuthorizationPhoneRideAlarmDeliverySpy: PhoneRideAlarmDelivering {
    private var authorizationContinuation: CheckedContinuation<PhoneRideAlarmAuthorization, Never>?

    var hasPendingAuthorizationRequest: Bool { authorizationContinuation != nil }

    func authorizationStatus() async -> PhoneRideAlarmAuthorization { .notDetermined }

    func requestAuthorization() async -> PhoneRideAlarmAuthorization {
        await withCheckedContinuation { authorizationContinuation = $0 }
    }

    func resolveAuthorization(_ authorization: PhoneRideAlarmAuthorization) {
        authorizationContinuation?.resume(returning: authorization)
        authorizationContinuation = nil
    }

    func deliver(_ request: MobilePhoneAlarmDeliveryRequestDto) async throws {}
    func cancel(requestIDs: [UInt64]) {}
}

@MainActor
private final class SuspendedPhoneRideAlarmDeliverySpy: PhoneRideAlarmDelivering {
    private var continuations = [UInt64: CheckedContinuation<Void, any Error>]()
    private(set) var pendingRequests = [MobilePhoneAlarmDeliveryRequestDto]()
    private(set) var cancelledRequestIDs = [UInt64]()

    func authorizationStatus() async -> PhoneRideAlarmAuthorization {
        .permitted(alerts: true, sounds: true, quietly: false)
    }

    func requestAuthorization() async -> PhoneRideAlarmAuthorization {
        .permitted(alerts: true, sounds: true, quietly: false)
    }

    func deliver(_ request: MobilePhoneAlarmDeliveryRequestDto) async throws {
        pendingRequests.append(request)
        try await withCheckedThrowingContinuation { continuations[request.id] = $0 }
    }

    func cancel(requestIDs: [UInt64]) {
        cancelledRequestIDs.append(contentsOf: requestIDs)
    }

    func resolveAllDeliveries() {
        let pending = continuations.values
        continuations.removeAll()
        pending.forEach { $0.resume(returning: ()) }
    }
}

@MainActor
private final class RefreshRacePhoneRideAlarmDeliverySpy: PhoneRideAlarmDelivering {
    private var authorizationStatusCalls = 0
    private var suspendedRefresh: CheckedContinuation<PhoneRideAlarmAuthorization, Never>?

    var hasSuspendedRefresh: Bool { suspendedRefresh != nil }

    func authorizationStatus() async -> PhoneRideAlarmAuthorization {
        authorizationStatusCalls += 1
        guard authorizationStatusCalls == 1 else {
            return .permitted(alerts: true, sounds: true, quietly: false)
        }
        return await withCheckedContinuation { suspendedRefresh = $0 }
    }

    func requestAuthorization() async -> PhoneRideAlarmAuthorization { .notDetermined }

    func resolveSuspendedRefresh(_ authorization: PhoneRideAlarmAuthorization) {
        suspendedRefresh?.resume(returning: authorization)
        suspendedRefresh = nil
    }

    func deliver(_ request: MobilePhoneAlarmDeliveryRequestDto) async throws {}
    func cancel(requestIDs: [UInt64]) {}
}
