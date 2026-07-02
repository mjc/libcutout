import CutoutMobile
import Foundation

final class LiveRideModel: ObservableObject {
    @Published private(set) var displayState = LiveRideDisplayState()
    @Published private(set) var phase = LiveRideConnectionPhase.starting
    @Published private(set) var devicePickerScanState: DevicePickerScanState?
    @Published private(set) var selectedRideTitle: String?

    var speed: SpeedReadout {
        displayState.speed
    }

    var rideState: EucRideScreenState {
        EucRideScreenState(phase: phase, displayState: displayState)
    }

    private let core = LiveRideSessionCore()

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
