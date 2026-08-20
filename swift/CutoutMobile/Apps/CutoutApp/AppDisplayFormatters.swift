import CutoutMobile

extension EucRideWarningSeverity {
    var accessibilityAnnouncement: String? {
        switch self {
        case .caution:
            localizedAppText("accessibility.euc_warning.caution")
        case .reduceAcceleration:
            localizedAppText("accessibility.euc_warning.reduce_acceleration")
        case .limpHome:
            localizedAppText("accessibility.euc_warning.limp_home")
        case .normal, .unavailable, .failed:
            nil
        }
    }
}

extension VescRideWarning {
    var accessibilityAnnouncement: String? {
        switch self {
        case .lowVoltage:
            localizedAppText("accessibility.vesc_warning.low_voltage")
        case .highVoltage:
            localizedAppText("accessibility.vesc_warning.high_voltage")
        case .mosfetTemperature:
            localizedAppText("accessibility.vesc_warning.mosfet_temperature")
        case .motorTemperature:
            localizedAppText("accessibility.vesc_warning.motor_temperature")
        case .current:
            localizedAppText("accessibility.vesc_warning.current")
        case .dutyPushback:
            localizedAppText("accessibility.vesc_warning.duty_pushback")
        case .temperaturePushback:
            localizedAppText("accessibility.vesc_warning.temperature_pushback")
        case .wheelslip:
            localizedAppText("accessibility.vesc_warning.wheelslip")
        case .sensors:
            localizedAppText("accessibility.vesc_warning.sensors")
        case .lowBattery:
            localizedAppText("accessibility.vesc_warning.low_battery")
        case .error:
            localizedAppText("accessibility.vesc_warning.error")
        case .none, .unknown:
            nil
        }
    }
}

extension VescRideStopReason {
    var accessibilityAnnouncement: String? {
        switch self {
        case .none: nil
        case .pitch: localizedAppText("accessibility.vesc_stop.pitch")
        case .roll: localizedAppText("accessibility.vesc_stop.roll")
        case .switchHalf: localizedAppText("accessibility.vesc_stop.switch_half")
        case .switchFull: localizedAppText("accessibility.vesc_stop.switch_full")
        case .reverse: localizedAppText("accessibility.vesc_stop.reverse")
        case .quickStop: localizedAppText("accessibility.vesc_stop.quick_stop")
        }
    }
}

extension BmsAlertLevel {
    var accessibilityAnnouncement: String? {
        switch self {
        case .warning:
            localizedAppText("accessibility.bms_alert.warning")
        case .critical:
            localizedAppText("accessibility.bms_alert.critical")
        case .nominal, .unknown:
            nil
        }
    }
}

extension BmsSnapshot {
    var accessibilityAlertLevel: BmsAlertLevel {
        if groups.contains(where: { $0.alertLevel == .critical }) {
            return .critical
        }
        if groups.contains(where: { $0.alertLevel == .warning }) {
            return .warning
        }
        if groups.contains(where: { $0.alertLevel == .nominal }) {
            return .nominal
        }
        return .unknown
    }
}
