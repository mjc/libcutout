import CutoutMobile
import Foundation

final class LiveSpeedModel: ObservableObject {
    @Published private(set) var displayState = LiveSpeedDisplayState()
    @Published private(set) var phase = LiveSpeedConnectionPhase.starting

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
    }

    func start() {
        core.start()
    }
}
