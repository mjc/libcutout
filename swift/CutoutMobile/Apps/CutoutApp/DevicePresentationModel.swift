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
    private(set) var savedPlatformIdentifier: String?
    @ObservationIgnored private var selectionRestoreTask: Task<Void, Never>?
    @ObservationIgnored private var nameRestoreTask: Task<Void, Never>?
    @ObservationIgnored private var pendingNameRestore: String?
    @ObservationIgnored private var namePersistenceTask: Task<Void, Never>?
    @ObservationIgnored private var pendingNamePersistence: (identity: String, name: String)?
    @ObservationIgnored private var activeNamePersistenceCompletion: DispatchSemaphore?
    private var selectionRestored: Bool

    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?

    init(
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
        restoredSelection: DevicePickerSelectionSnapshot? = nil
    ) {
        self.selectedDeviceStore = selectedDeviceStore
        let selection = restoredSelection ?? selectedDeviceStore.immediateSelection
        selectionRestored = restoredSelection != nil || !selectedDeviceStore.requiresDatabaseLoad
        savedPlatformIdentifier = selection.platformIdentifier
        if let identity = selection.platformIdentifier, let name = selection.displayName {
            vehicleNameCache[identity] = name
        }
        hasSavedDevice = selection.platformIdentifier != nil
    }

    deinit {
        selectionRestoreTask?.cancel()
        nameRestoreTask?.cancel()
        namePersistenceTask?.cancel()
    }

    @discardableResult
    func restoreSavedSelection(onRestored: @escaping @MainActor () -> Void = {}) -> Task<Void, Never>? {
        guard !selectionRestored else { return nil }
        if let selectionRestoreTask { return selectionRestoreTask }
        let store = selectedDeviceStore
        let task = Task { [weak self, store] in
            let selection = await store.load()
            guard let self, !Task.isCancelled else { return }
            selectionRestoreTask = nil
            selectionRestored = true
            savedPlatformIdentifier = selection.platformIdentifier
            hasSavedDevice = selection.platformIdentifier != nil
            if let identity = selection.platformIdentifier, let name = selection.displayName,
                vehicleNameCache[identity] == nil
            {
                vehicleNameCache[identity] = name
            }
            onRestored()
        }
        selectionRestoreTask = task
        return task
    }

    func restoreVehicleName(for identity: String) {
        guard vehicleName(for: identity) == nil else { return }
        pendingNameRestore = identity
        guard nameRestoreTask == nil else { return }
        let store = selectedDeviceStore
        nameRestoreTask = Task { [weak self, store] in
            while let identity = self?.takePendingNameRestore() {
                let name = await store.loadDisplayName(for: identity)
                guard !Task.isCancelled else { return }
                self?.applyRestoredName(name, for: identity)
            }
        }
    }

    private func takePendingNameRestore() -> String? {
        let identity = pendingNameRestore
        pendingNameRestore = nil
        if identity == nil { nameRestoreTask = nil }
        return identity
    }

    private func applyRestoredName(_ name: String?, for identity: String) {
        guard let name, vehicleNameCache[identity] == nil else { return }
        vehicleNameCache[identity] = name
        if let selection = connectionState.selection,
            selection.platformIdentifier == identity,
            selection.title == localizedAppText("setup.device")
        {
            connectionState = connectionState.replacingSelection(
                with: ConnectionSelection(
                    platformIdentifier: identity, title: name, route: selection.route))
        }
    }

    func waitForVehicleNameRestoration() async {
        await nameRestoreTask?.value
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
        connectionState.selection?.platformIdentifier ?? savedPlatformIdentifier
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
        vehicleName(for: identity)
    }

    func rememberVehicleName(_ name: String, for identity: String) {
        vehicleNameCache[identity] = name
        selectedDeviceStore.save(platformIdentifier: identity, displayName: name)
        savedPlatformIdentifier = identity
    }

    @discardableResult
    func applyProtocolIdentityCandidate(
        _ candidate: DevicePickerDiscoveryCandidate?,
        allowsRidePresentation: Bool
    ) -> Bool {
        if let candidate, let selectedIdentifier = connectionState.selection?.platformIdentifier,
            selectedIdentifier != candidate.platformIdentifier
        {
            return false
        }
        protocolIdentityCandidate = candidate
        guard allowsRidePresentation, let candidate else { return true }

        guard let selection = Self.connectionSelection(from: candidate) else { return true }

        if let displayName = Self.meaningfulDeviceName(
            candidate.displayName,
            identity: candidate.platformIdentifier
        ), persistedVehicleName(for: candidate.platformIdentifier) != displayName {
            rememberProtocolVehicleName(displayName, for: candidate.platformIdentifier)
        }

        let resolvedSelection = ConnectionSelection(
            platformIdentifier: selection.platformIdentifier,
            title: connectionState.selection?.title ?? selection.title,
            route: selection.route
        )
        connectionState = connectionState.replacingSelection(with: resolvedSelection)
        return true
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
        persistAcceptedSelection: Bool = true,
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
        protocolIdentityCandidate = nil
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
        guard persistAcceptedSelection else {
            hasSavedDevice = savedPlatformIdentifier != nil
            return .accepted(selectedRow)
        }
        retireSelectionRestoration()
        let selectionChanged = savedPlatformIdentifier != platformIdentifier
        if selectionChanged {
            if let displayName {
                rememberVehicleName(displayName, for: platformIdentifier)
            } else {
                selectedDeviceStore.save(platformIdentifier: platformIdentifier)
                savedPlatformIdentifier = platformIdentifier
            }
        } else if let displayName, displayName != persistedDisplayName {
            rememberVehicleName(displayName, for: platformIdentifier)
        }
        hasSavedDevice = true
        return .accepted(selectedRow)
    }

    func forgetSavedDevice() {
        retireSelectionRestoration()
        try? selectedDeviceStore.clear()
        savedPlatformIdentifier = nil
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
        guard let name = selectedDeviceStore.immediateDisplayName(for: identity) else { return nil }
        vehicleNameCache[identity] = name
        return name
    }

    private func retireSelectionRestoration() {
        // Pair/forget already owns synchronous persistence. Finish the off-Main name
        // receipt before settling its latest pending value at an identity change.
        if let completion = activeNamePersistenceCompletion {
            completion.wait()
            completion.signal()
        }
        if let effect = pendingNamePersistence {
            selectedDeviceStore.saveDisplayName(effect.name, for: effect.identity)
            pendingNamePersistence = nil
        }
        selectionRestoreTask?.cancel()
        selectionRestoreTask = nil
        selectionRestored = true
    }

    private func rememberProtocolVehicleName(_ name: String, for identity: String) {
        vehicleNameCache[identity] = name
        guard selectedDeviceStore.requiresDatabaseLoad else {
            selectedDeviceStore.saveDisplayName(name, for: identity)
            return
        }
        pendingNamePersistence = (identity, name)
        guard namePersistenceTask == nil else { return }
        let store = selectedDeviceStore
        namePersistenceTask = Task { [weak self, store] in
            while let effect = self?.takePendingNamePersistence() {
                guard !Task.isCancelled else { return }
                let completion = DispatchSemaphore(value: 0)
                self?.activeNamePersistenceCompletion = completion
                await store.saveDisplayNameAsync(effect.name, for: effect.identity) { completion.signal() }
                self?.activeNamePersistenceCompletion = nil
            }
        }
    }

    private func takePendingNamePersistence() -> (identity: String, name: String)? {
        let effect = pendingNamePersistence
        pendingNamePersistence = nil
        if effect == nil { namePersistenceTask = nil }
        return effect
    }

    func waitForProtocolNamePersistence() async {
        await namePersistenceTask?.value
    }

    static func meaningfulDeviceName(_ candidate: String?, identity: String) -> String? {
        guard let candidate, !candidate.isEmpty, candidate != identity else { return nil }
        return candidate
    }
}
