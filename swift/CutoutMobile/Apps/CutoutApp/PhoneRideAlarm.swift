import CutoutMobile
import CutoutMobileFFI
import Foundation
@preconcurrency import UserNotifications
#if canImport(UIKit)
import UIKit
#endif

enum PhoneRideAlarmAuthorization: Equatable, Sendable {
    case unavailable
    case notDetermined
    case denied
    case permitted(alerts: Bool, sounds: Bool, quietly: Bool)

    var capability: MobilePhoneAlarmDeliveryCapabilityDto {
        switch self {
        case let .permitted(alerts, sounds, quietly):
            MobilePhoneAlarmDeliveryCapabilityDto(
                canSchedule: alerts || sounds,
                playsSound: sounds && !quietly
            )
        case .unavailable, .notDetermined, .denied:
            MobilePhoneAlarmDeliveryCapabilityDto(canSchedule: false, playsSound: false)
        }
    }
}

@MainActor
protocol PhoneRideAlarmDelivering: AnyObject {
    func authorizationStatus() async -> PhoneRideAlarmAuthorization
    func requestAuthorization() async -> PhoneRideAlarmAuthorization
    func deliver(_ request: MobilePhoneAlarmDeliveryRequestDto) async throws
    func cancel(requestIDs: [UInt64])
}

@MainActor
func makePhoneRideAlarmDelivery() -> any PhoneRideAlarmDelivering {
#if os(iOS)
    SystemPhoneRideAlarmDelivery()
#else
    UnavailablePhoneRideAlarmDelivery()
#endif
}

@MainActor
private final class UnavailablePhoneRideAlarmDelivery: PhoneRideAlarmDelivering {
    func authorizationStatus() async -> PhoneRideAlarmAuthorization { .unavailable }
    func requestAuthorization() async -> PhoneRideAlarmAuthorization { .unavailable }
    func deliver(_ request: MobilePhoneAlarmDeliveryRequestDto) async throws {}
    func cancel(requestIDs: [UInt64]) {}
}

@MainActor
final class SystemPhoneRideAlarmDelivery: PhoneRideAlarmDelivering {
    private let center: UNUserNotificationCenter
    private let notificationDelegate: PhoneRideAlarmNotificationDelegate
    private let notificationIdentifierPrefix = "phone-ride-alarm-\(UUID().uuidString)-"

    init(center: UNUserNotificationCenter = .current()) {
        self.center = center
        notificationDelegate = PhoneRideAlarmNotificationDelegate()
        center.delegate = notificationDelegate
    }

    func authorizationStatus() async -> PhoneRideAlarmAuthorization {
        await center.notificationSettings().phoneRideAlarmAuthorization
    }

    func requestAuthorization() async -> PhoneRideAlarmAuthorization {
        do {
            _ = try await center.requestAuthorization(options: [.alert, .sound])
        } catch {
            return .denied
        }
        return await authorizationStatus()
    }

    func deliver(_ request: MobilePhoneAlarmDeliveryRequestDto) async throws {
        let alert = PhoneRideAlarmAlert(event: request.event)
        let content = UNMutableNotificationContent()
        content.title = alert.title
        content.body = alert.body
        if request.playsSound {
            content.sound = .default
        }
        try await center.add(
            UNNotificationRequest(
                identifier: phoneRideAlarmIdentifier(
                    request.id,
                    prefix: notificationIdentifierPrefix
                ),
                content: content,
                trigger: nil
            )
        )
#if canImport(UIKit)
        if request.playsSound, UIApplication.shared.applicationState == .active {
            UINotificationFeedbackGenerator().notificationOccurred(.warning)
        }
#endif
    }

    func cancel(requestIDs: [UInt64]) {
        let identifiers = requestIDs.map {
            phoneRideAlarmIdentifier($0, prefix: notificationIdentifierPrefix)
        }
        center.removePendingNotificationRequests(withIdentifiers: identifiers)
        center.removeDeliveredNotifications(withIdentifiers: identifiers)
    }
}

private func phoneRideAlarmIdentifier(_ requestID: UInt64, prefix: String) -> String {
    "\(prefix)\(requestID)"
}

struct PhoneRideAlarmAlert: Equatable {
    let title: String
    let body: String

    init(event: MobilePhoneAlarmEventDto) {
        switch event {
        case let .pwmDuty(dutyPercent, headroomPercent):
            title = localizedAppText("phone_alarm.notification.pwm_title")
            body = localizedAppText(
                "phone_alarm.notification.pwm_body",
                Int64(dutyPercent),
                Int64(headroomPercent)
            )
        case let .controllerWarning(warning):
            title = localizedAppText("phone_alarm.notification.controller_warning_title")
            body = phoneRideAlarmWarningAnnouncement(warning)
                ?? localizedAppText("phone_alarm.notification.controller_warning_unknown")
        case let .controllerStop(reason):
            title = localizedAppText("phone_alarm.notification.controller_stop_title")
            body = phoneRideAlarmStopAnnouncement(reason)
                ?? localizedAppText("phone_alarm.notification.controller_stop_unknown")
        }
    }
}

private func phoneRideAlarmWarningAnnouncement(_ warning: MobileVescRideWarningDto) -> String? {
    switch warning {
    case .lowVoltage: localizedAppText("accessibility.vesc_warning.low_voltage")
    case .highVoltage: localizedAppText("accessibility.vesc_warning.high_voltage")
    case .mosfetTemperature: localizedAppText("accessibility.vesc_warning.mosfet_temperature")
    case .motorTemperature: localizedAppText("accessibility.vesc_warning.motor_temperature")
    case .current: localizedAppText("accessibility.vesc_warning.current")
    case .dutyPushback: localizedAppText("accessibility.vesc_warning.duty_pushback")
    case .speedPushback: localizedAppText("accessibility.vesc_warning.speed_pushback")
    case .temperaturePushback: localizedAppText("accessibility.vesc_warning.temperature_pushback")
    case .wheelslip: localizedAppText("accessibility.vesc_warning.wheelslip")
    case .sensors: localizedAppText("accessibility.vesc_warning.sensors")
    case .lowBattery: localizedAppText("accessibility.vesc_warning.low_battery")
    case .error: localizedAppText("accessibility.vesc_warning.error")
    case .bmsConnection: localizedAppText("accessibility.vesc_warning.bms_connection")
    case .none, .unknown: nil
    }
}

private func phoneRideAlarmStopAnnouncement(_ reason: MobileVescRideStopReasonDto) -> String? {
    switch reason {
    case .none: nil
    case .pitch: localizedAppText("accessibility.vesc_stop.pitch")
    case .roll: localizedAppText("accessibility.vesc_stop.roll")
    case .switchHalf: localizedAppText("accessibility.vesc_stop.switch_half")
    case .switchFull: localizedAppText("accessibility.vesc_stop.switch_full")
    case .reverse: localizedAppText("accessibility.vesc_stop.reverse")
    case .quickStop: localizedAppText("accessibility.vesc_stop.quick_stop")
    }
}

private final class PhoneRideAlarmNotificationDelegate: NSObject, UNUserNotificationCenterDelegate {
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        guard notification.request.identifier.hasPrefix("phone-ride-alarm-") else {
            completionHandler([])
            return
        }
        let options: UNNotificationPresentationOptions =
            notification.request.content.sound == nil ? [.banner] : [.banner, .sound]
        completionHandler(options)
    }
}

private extension UNNotificationSettings {
    var phoneRideAlarmAuthorization: PhoneRideAlarmAuthorization {
        switch authorizationStatus {
        case .notDetermined:
            .notDetermined
        case .denied:
            .denied
        case .authorized:
            .permitted(
                alerts: alertSetting == .enabled,
                sounds: soundSetting == .enabled,
                quietly: false
            )
        case .provisional, .ephemeral:
            .permitted(
                alerts: alertSetting == .enabled,
                sounds: soundSetting == .enabled,
                quietly: true
            )
        @unknown default:
            .unavailable
        }
    }
}
