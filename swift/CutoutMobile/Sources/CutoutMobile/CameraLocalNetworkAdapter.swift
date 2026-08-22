import Foundation
import Network
import Observation

/// Coarse local-network path state needed before camera discovery.
public enum CameraLocalNetworkPathStatus: Equatable, Sendable {
    case unavailable
    case satisfied
}

/// Maps Apple path evidence to a conservative camera connection state.
public func cameraConnectionPresentation(
    pathStatus: CameraLocalNetworkPathStatus,
    usesWiFi: Bool
) -> CameraConnectionPresentation {
    guard pathStatus == .satisfied, usesWiFi else { return .wifiRequired }
    return .notConfigured
}

/// Apple-owned local-network readiness monitor for the selected camera path.
///
/// This monitor does not scan the LAN or infer a camera connection. It only
/// reports whether the phone currently has a usable Wi-Fi path; a future
/// selected-origin connection will supply the read-only Novatek evidence.
@MainActor
@Observable
public final class CameraLocalNetworkAdapter {
    public private(set) var presentation: CameraPresentation

    private var monitor: NWPathMonitor?
    private let monitorQueue = DispatchQueue(label: "org.cutout.camera-local-network")

    public init(presentation: CameraPresentation = .initial) {
        self.presentation = presentation
    }

    /// Starts observing the Wi-Fi path without adding a timeout or scanner.
    public func start() {
        guard monitor == nil else { return }

        let monitor = NWPathMonitor(requiredInterfaceType: .wifi)
        monitor.pathUpdateHandler = { [weak self] path in
            let status: CameraLocalNetworkPathStatus = path.status == .satisfied
                ? .satisfied
                : .unavailable
            let usesWiFi = path.usesInterfaceType(.wifi)
            Task { @MainActor [weak self] in
                self?.apply(pathStatus: status, usesWiFi: usesWiFi)
            }
        }
        monitor.start(queue: monitorQueue)
        self.monitor = monitor
    }

    /// Stops observing the path and returns to a non-optimistic initial state.
    public func stop() {
        monitor?.cancel()
        monitor = nil
        presentation = .initial
    }

    private func apply(pathStatus: CameraLocalNetworkPathStatus, usesWiFi: Bool) {
        presentation.connection = cameraConnectionPresentation(
            pathStatus: pathStatus,
            usesWiFi: usesWiFi
        )
    }
}
