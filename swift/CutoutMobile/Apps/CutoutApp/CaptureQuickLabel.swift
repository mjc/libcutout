import CutoutMobileFFI

/// Native display vocabulary; annotation strings and exclusivity are owned by Rust.
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

    var domainLabel: MobileCaptureLabelDto {
        switch self {
        case .ride: .ride
        case .charge: .charging
        case .balance: .balancing
        case .lowBeamOn: .lowBeamOn
        case .lowBeamOff: .lowBeamOff
        case .highBeamOn: .highBeamOn
        case .highBeamOff: .highBeamOff
        case .horn: .horn
        case .pedalsHard: .pedalsHard
        case .pedalsMedium: .pedalsMedium
        case .pedalsSoft: .pedalsSoft
        case .resetTrip: .resetTrip
        case .softwareLock: .softwareLock
        case .softwareUnlock: .softwareUnlock
        case .tiltbackSpeed: .tiltbackSpeed
        case .alarmSpeed: .alarmSpeed
        case .angleAdjustment: .angleAdjustment
        case .rideMode: .rideMode
        case .pwmPercent: .pwmPercent
        }
    }

    func actionTitle(isActive: Bool) -> String {
        localizedAppText(isActive ? "capture.label.stop" : "capture.label.start", title)
    }

    var title: String { localizedAppText("capture.label.\(annotationValue)") }
    var annotationValue: String { captureLabelSlug(label: domainLabel) }

    func isMutuallyExclusive(with other: Self) -> Bool {
        captureLabelsAreMutuallyExclusive(left: domainLabel, right: other.domainLabel)
    }
}
