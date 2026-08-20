private enum CaptureLabelExclusiveGroup {
    case lowBeam
    case highBeam
    case pedalMode
    case softwareLock
}

enum CaptureQuickLabel: CaseIterable, Hashable, Identifiable {
    case ride
    case charge
    case balance
    case lowBeamOn
    case lowBeamOff
    case highBeamOn
    case highBeamOff
    case horn
    case pedalsHard
    case pedalsMedium
    case pedalsSoft
    case resetTrip
    case softwareLock
    case softwareUnlock
    case tiltbackSpeed
    case alarmSpeed
    case angleAdjustment
    case rideMode
    case pwmPercent

    var id: String { annotationValue }

    func actionTitle(isActive: Bool) -> String {
        localizedAppText(isActive ? "capture.label.stop" : "capture.label.start", title)
    }

    var title: String {
        localizedAppText("capture.label.\(annotationValue)")
    }

    var annotationValue: String {
        switch self {
        case .ride:
            "ride"
        case .charge:
            "charging"
        case .balance:
            "balancing"
        case .lowBeamOn:
            "low_beam_on"
        case .lowBeamOff:
            "low_beam_off"
        case .highBeamOn:
            "high_beam_on"
        case .highBeamOff:
            "high_beam_off"
        case .horn:
            "horn"
        case .pedalsHard:
            "pedals_hard"
        case .pedalsMedium:
            "pedals_medium"
        case .pedalsSoft:
            "pedals_soft"
        case .resetTrip:
            "reset_trip"
        case .softwareLock:
            "software_lock"
        case .softwareUnlock:
            "software_unlock"
        case .tiltbackSpeed:
            "tiltback_speed"
        case .alarmSpeed:
            "alarm_speed"
        case .angleAdjustment:
            "angle_adjustment"
        case .rideMode:
            "ride_mode"
        case .pwmPercent:
            "pwm_percent"
        }
    }

    func isMutuallyExclusive(with other: Self) -> Bool {
        guard let exclusiveGroup else { return false }
        return other.exclusiveGroup == exclusiveGroup
    }

    private var exclusiveGroup: CaptureLabelExclusiveGroup? {
        switch self {
        case .lowBeamOn, .lowBeamOff:
            .lowBeam
        case .highBeamOn, .highBeamOff:
            .highBeam
        case .pedalsHard, .pedalsMedium, .pedalsSoft:
            .pedalMode
        case .softwareLock, .softwareUnlock:
            .softwareLock
        default:
            nil
        }
    }
}
