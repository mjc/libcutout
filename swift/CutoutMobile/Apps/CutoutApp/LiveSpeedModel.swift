import CutoutMobile
import Foundation

final class LiveSpeedModel: ObservableObject {
    @Published private(set) var displayState = LiveSpeedDisplayState()
    @Published private(set) var phase = LiveSpeedConnectionPhase.starting
    @Published private(set) var devicePickerScanState: DevicePickerScanState?

    var speed: SpeedReadout {
        displayState.speed
    }

    private let core = LiveSpeedSessionCore()

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
}
