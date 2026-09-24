import Foundation

/// The side effects sequenced around one Rust-produced notification step.
///
/// The production implementation remains owned by `CutoutSessionCore`; tests can inject a
/// recorder without constructing CoreBluetooth, Core Location, or a ride-map database.
struct CutoutSessionNotificationEffects {
    let applyActions: ([SessionAction]) -> Void
    let observeRideMapConnection: (MonotonicMilliseconds) -> Void
    let persistBmsSamples: ([BmsRawVoltageObservation]) -> Void
    let reduceDisplayState: (RideDisplayState, TelemetrySnapshot?, MonotonicMilliseconds) -> RideDisplayState

    init(
        applyActions: @escaping ([SessionAction]) -> Void,
        observeRideMapConnection: @escaping (MonotonicMilliseconds) -> Void,
        persistBmsSamples: @escaping ([BmsRawVoltageObservation]) -> Void,
        reduceDisplayState: @escaping (RideDisplayState, TelemetrySnapshot?, MonotonicMilliseconds) -> RideDisplayState
    ) {
        self.applyActions = applyActions
        self.observeRideMapConnection = observeRideMapConnection
        self.persistBmsSamples = persistBmsSamples
        self.reduceDisplayState = reduceDisplayState
    }
}
