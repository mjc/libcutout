import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

enum DevicePairOutcome {
    case ignored
    case unavailable
    case accepted(DevicePickerRow)
    case refused
}

/// App-lifetime presentation of the one Rust-backed device session.
@MainActor
@Observable
final class DevicePresentationModel {
    private let selectedDeviceStore: DevicePickerSelectionStore
    private var vehicleNameCache = [String: String]()

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

    func applyProtocolIdentityCandidate(
        _ candidate: DevicePickerDiscoveryCandidate?,
        allowsRidePresentation: Bool
    ) {
        protocolIdentityCandidate = candidate
        guard allowsRidePresentation, let candidate else { return }

        if let displayName = Self.meaningfulDeviceName(
            candidate.displayName,
            identity: candidate.platformIdentifier
        ), persistedVehicleName(for: candidate.platformIdentifier) != displayName {
            rememberVehicleName(displayName, for: candidate.platformIdentifier)
        }

        guard let selection = Self.connectionSelection(from: candidate) else { return }
        guard
            connectionState.selection?.platformIdentifier == nil
                || connectionState.selection?.platformIdentifier == selection.platformIdentifier
        else { return }

        let resolvedSelection = ConnectionSelection(
            platformIdentifier: selection.platformIdentifier,
            title: connectionState.selection?.title ?? selection.title,
            route: selection.route
        )
        connectionState = connectionState.replacingSelection(with: resolvedSelection)
    }

    static func connectionSelection(from candidate: DevicePickerDiscoveryCandidate?) -> ConnectionSelection? {
        guard let candidate, candidate.support.isSupported, let route = candidate.support.connectionRoute else {
            return nil
        }
        return ConnectionSelection(
            platformIdentifier: candidate.platformIdentifier,
            title: candidate.displayName,
            route: route
        )
    }

    func pair(
        platformIdentifier: String,
        mayRetryCurrentSelection: Bool,
        performPair: (DevicePickerRow) -> Bool
    ) -> DevicePairOutcome {
        switch connectionState {
        case .connecting, .retrying, .connected:
            let isSameSelection = connectionState.selection?.platformIdentifier == platformIdentifier
            guard !isSameSelection || mayRetryCurrentSelection else { return .ignored }
        case .picker, .identified, .failed:
            break
        }

        let rows = scanState?.rows ?? []
        guard let selectedRow = rows.first(where: { $0.id == platformIdentifier }) else {
            phase = .scanning
            scanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
            return .unavailable
        }
        guard selectedRow.isSupported || selectedRow.isProbeRecommended else { return .ignored }

        let selection = ConnectionSelection(
            platformIdentifier: selectedRow.id,
            title: selectedRow.title,
            route: selectedRow.connectionRoute ?? .electricUnicycle
        )
        settings = nil
        connectionState = .connecting(selection, phase: .discoveringServices)
        phase = .discoveringServices

        guard performPair(selectedRow) else {
            connectionState = .picker
            phase = .scanning
            scanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
            return .refused
        }

        let displayName = Self.meaningfulDeviceName(
            selectedRow.title,
            identity: platformIdentifier
        )
        let persistedDisplayName = persistedVehicleName(for: platformIdentifier)
        let selectionChanged = selectedDeviceStore.platformIdentifier != platformIdentifier
        if selectionChanged {
            if let displayName {
                rememberVehicleName(displayName, for: platformIdentifier)
            } else {
                selectedDeviceStore.save(platformIdentifier: platformIdentifier)
            }
        } else if let displayName, displayName != persistedDisplayName {
            rememberVehicleName(displayName, for: platformIdentifier)
        }
        hasSavedDevice = true
        return .accepted(selectedRow)
    }

    func forgetSavedDevice() {
        try? selectedDeviceStore.clear()
        hasSavedDevice = false
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
        if let name = vehicleNameCache[identity] {
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
