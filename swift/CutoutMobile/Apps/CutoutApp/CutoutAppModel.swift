import CutoutMobile
import Foundation

<<<<<<<< HEAD:swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift
final class CutoutAppModel: ObservableObject {
    @Published private(set) var displayState = RideDisplayState()
    @Published private(set) var phase = SessionConnectionPhase.starting
|||||||| 2bdc2c8e:swift/CutoutMobile/Apps/CutoutApp/LiveSpeedModel.swift
final class LiveSpeedModel: ObservableObject {
    @Published private(set) var displayState = LiveSpeedDisplayState()
    @Published private(set) var phase = LiveSpeedConnectionPhase.starting
========
final class LiveRideModel: ObservableObject {
    @Published private(set) var displayState = LiveRideDisplayState()
    @Published private(set) var phase = LiveRideConnectionPhase.starting
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Apps/CutoutApp/LiveRideModel.swift
    @Published private(set) var devicePickerScanState: DevicePickerScanState?
    @Published private(set) var selectedRideTitle: String?
    @Published private(set) var settingsReadback: SettingsReadback?
    @Published private(set) var faultHistoryReadback: FaultHistoryReadback?
    @Published private(set) var bmsSnapshot: BmsSnapshot?

    var speed: SpeedReadout {
        displayState.speed
    }

    var rideState: EucRideScreenState {
        EucRideScreenState(phase: phase, displayState: displayState)
    }

<<<<<<<< HEAD:swift/CutoutMobile/Apps/CutoutApp/CutoutAppModel.swift
    private let core = CutoutSessionCore()
|||||||| 2bdc2c8e:swift/CutoutMobile/Apps/CutoutApp/LiveSpeedModel.swift
    private let core = LiveSpeedSessionCore()
========
    private let core = LiveRideSessionCore()
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Apps/CutoutApp/LiveRideModel.swift

    init() {
        core.onDisplayStateChange = { [weak self] displayState in
            self?.displayState = displayState
        }
        core.onPhaseChange = { [weak self] phase in
            self?.phase = phase
        }
        core.onScanStateChange = { [weak self] scanState in
            self?.devicePickerScanState = scanState
        }
        core.onSettingsReadbackChange = { [weak self] settingsReadback in
            self?.settingsReadback = settingsReadback
        }
        core.onFaultHistoryReadbackChange = { [weak self] faultHistoryReadback in
            self?.faultHistoryReadback = faultHistoryReadback
        }
        core.onBmsSnapshotChange = { [weak self] bmsSnapshot in
            self?.bmsSnapshot = bmsSnapshot
        }
        core.onProtocolIdentityCandidateChange = { [weak self] candidate in
            guard case .supported = candidate?.support else { return }
            self?.selectedRideTitle = candidate?.detail
        }
    }

    deinit {}

    func start() {
        core.start()
    }

    func pair(platformIdentifier: String) -> Bool {
        let didPair = core.pair(platformIdentifier: platformIdentifier)
        if didPair {
            selectedRideTitle = devicePickerScanState?.rows.first(where: { $0.id == platformIdentifier })?.title
        }
        return didPair
    }

    func disconnectAndSearch() {
        selectedRideTitle = nil
        core.disconnectAndScan()
    }
}
