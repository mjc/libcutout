import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// App-lifetime presentation of the one Rust-backed device session.
@MainActor
@Observable
final class DevicePresentationModel {
    private let selectedDeviceStore: DevicePickerSelectionStore
    private var vehicleNameCache = [String: String]()
    private var historicalVehicleNames = [String: String]()

    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?

    init(selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore()) {
        self.selectedDeviceStore = selectedDeviceStore
        hasSavedDevice = selectedDeviceStore.platformIdentifier != nil
    }

    var displayState = RideDisplayState()
    var phase = SessionConnectionPhase.starting
    var scanState: DevicePickerScanState?
    var connectionState = ConnectionState.picker
    var faultHistoryReadback: FaultHistoryReadback?
    var bmsSnapshot: BmsSnapshot?
    var phoneLocationReadback = PhoneLocationReadback(
        snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    )
    var hasSavedDevice = false
    var settings: DeviceSettings?

    var rideMapVehicleIdentity: String? {
        connectionState.selection?.platformIdentifier ?? selectedDeviceStore.platformIdentifier
    }

    var rideMapVehicleName: String? {
        guard let identity = rideMapVehicleIdentity else {
            return connectionState.selection?.title
        }
        if let name = vehicleName(for: identity) {
            return name
        }
        if let candidate = protocolIdentityCandidate,
            candidate.platformIdentifier == identity,
            candidate.displayName != identity,
            !candidate.displayName.isEmpty
        {
            return candidate.displayName
        }
        return Self.meaningfulDeviceName(connectionState.selection?.title, identity: identity)
            ?? Self.meaningfulDeviceName(
                scanState?.rows.first(where: { $0.id == identity })?.title,
                identity: identity
            )
    }

    func rideMapVehicleName(for identity: String?) -> String? {
        guard let identity else { return nil }
        if let name = vehicleName(for: identity) {
            return name
        }
        return identity == rideMapVehicleIdentity ? rideMapVehicleName : nil
    }

    func persistedVehicleName(for identity: String) -> String? {
        selectedDeviceStore.displayName(for: identity)
    }

    func rememberVehicleName(_ name: String, for identity: String) {
        vehicleNameCache[identity] = name
        selectedDeviceStore.save(platformIdentifier: identity, displayName: name)
    }

    func mergeHistoricalVehicleNames(_ names: [String: String]) {
        historicalVehicleNames.merge(names, uniquingKeysWith: { _, incoming in incoming })
        vehicleNameCache.merge(names, uniquingKeysWith: { _, incoming in incoming })
    }

    var selectedRideTitle: String? { connectionState.selection?.title }
    var selectedRideIdentifier: String? { connectionState.selection?.platformIdentifier }
    var selectedConnectionRoute: DevicePickerConnectionRoute? { connectionState.selection?.route }
    var speed: SpeedReadout { displayState.speed }
    var rideState: EucRideScreenState { EucRideScreenState(phase: phase, displayState: displayState) }
    var eucRidePresentationState: EucRideScreenState? {
        guard selectedRideTitle != nil || phase != .starting || displayState.notificationCount != 0 else {
            return nil
        }
        return rideState
    }
    var vescRideSnapshot: VescRideSnapshot? {
        VescRideSnapshot(displayState: displayState, title: selectedRideTitle)
    }
    var connectionStatusText: String { connectionState.statusText ?? phase.displayText }

    private func vehicleName(for identity: String) -> String? {
        if let name = historicalVehicleNames[identity] ?? vehicleNameCache[identity] {
            return name
        }
        guard let name = selectedDeviceStore.displayName(for: identity) else { return nil }
        vehicleNameCache[identity] = name
        return name
    }

    static func meaningfulDeviceName(_ candidate: String?, identity: String) -> String? {
        guard let candidate, !candidate.isEmpty, candidate != identity else { return nil }
        return candidate
    }
}
