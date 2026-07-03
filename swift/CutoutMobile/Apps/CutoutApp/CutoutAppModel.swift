import CutoutMobile
import Foundation

final class CutoutAppModel: ObservableObject {
    @Published private(set) var displayState = RideDisplayState()
    @Published private(set) var phase = SessionConnectionPhase.starting
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

    private let core = CutoutSessionCore()

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
    }

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
