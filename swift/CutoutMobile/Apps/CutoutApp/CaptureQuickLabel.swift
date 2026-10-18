import CutoutMobileFFI

/// Native display vocabulary; annotation strings and exclusivity are owned by Rust.
enum CaptureQuickLabel: CaseIterable, Hashable, Identifiable {
    case poweredOnStationary
    case rollingForward
    case rollingBackward
    case liftedWheel
    case disconnectReconnect
    case powerCycle
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
    case alarmMode
    case angleAdjustment
    case rideMode
    case pwmPercent
    case pedalHardness
    case displayBrightness
    case dynamicAssist
    case pedalDipCompensation
    case lateralCutoff
    case voltageCorrection
    case displayUnits
    case brakeOverpressureAlarm
    case beeperVolume
    case rollAngleMode
    case ledPattern

    static let activity: [Self] = [
        .poweredOnStationary, .balance, .rollingForward, .rollingBackward, .liftedWheel, .ride, .charge,
    ]
    static var actions: [Self] { allCases.filter { !activity.contains($0) } }

    var id: String { annotationValue }

    var domainLabel: MobileCaptureLabelDto {
        switch self {
        case .poweredOnStationary: .poweredOnStationary
        case .rollingForward: .rollingForward
        case .rollingBackward: .rollingBackward
        case .liftedWheel: .liftedWheel
        case .disconnectReconnect: .disconnectReconnect
        case .powerCycle: .powerCycle
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
        case .alarmMode: .alarmChange
        case .angleAdjustment: .angleAdjustment
        case .rideMode: .rideMode
        case .pwmPercent: .pwmPercent
        case .pedalHardness: .pedalHardness
        case .displayBrightness: .displayBrightness
        case .dynamicAssist: .dynamicAssist
        case .pedalDipCompensation: .pedalDipCompensation
        case .lateralCutoff: .lateralCutoff
        case .voltageCorrection: .voltageCorrection
        case .displayUnits: .displayUnits
        case .brakeOverpressureAlarm: .brakeOverpressureAlarm
        case .beeperVolume: .beeperVolume
        case .rollAngleMode: .rollAngleMode
        case .ledPattern: .ledPattern
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
