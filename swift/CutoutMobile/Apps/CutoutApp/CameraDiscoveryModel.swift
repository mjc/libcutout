import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

@MainActor
@Observable
final class CameraDiscoveryModel {
    typealias EvidenceLoader = @MainActor (String, UInt16) async throws -> CameraReadOnlyEvidence

    private(set) var isReading = false
    private(set) var readErrorKey: String?
    let origin = mobileNovatekR3ProOrigin()

    @ObservationIgnored private let loadMediaEvidence: EvidenceLoader
    @ObservationIgnored private let loadEvidence: EvidenceLoader
    @ObservationIgnored private let observePermissionRequired: @MainActor () -> Void
    @ObservationIgnored private let prepareForEvidenceRefresh: @MainActor () -> Void
    @ObservationIgnored private let annotateCapture: ((String, String) -> Void)?
    @ObservationIgnored private var readTask: Task<Void, Never>?
    @ObservationIgnored private var readGeneration: UInt64 = 0

    init(
        adapter: CameraLocalNetworkAdapter,
        prepareForEvidenceRefresh: @escaping @MainActor () -> Void,
        annotateCapture: ((String, String) -> Void)?
    ) {
        self.loadEvidence = { address, port in
            try await adapter.loadReadOnlyEvidence(address: address, port: port, includeMedia: false)
        }
        self.loadMediaEvidence = { address, port in
            try await adapter.loadReadOnlyEvidence(address: address, port: port)
        }
        self.observePermissionRequired = adapter.observePermissionRequired
        self.prepareForEvidenceRefresh = prepareForEvidenceRefresh
        self.annotateCapture = annotateCapture
    }

    init(
        loadEvidence: @escaping EvidenceLoader,
        observePermissionRequired: @escaping @MainActor () -> Void = {},
        prepareForEvidenceRefresh: @escaping @MainActor () -> Void = {},
        annotateCapture: ((String, String) -> Void)? = nil
    ) {
        self.loadEvidence = loadEvidence
        self.loadMediaEvidence = loadEvidence
        self.observePermissionRequired = observePermissionRequired
        self.prepareForEvidenceRefresh = prepareForEvidenceRefresh
        self.annotateCapture = annotateCapture
    }

    @discardableResult
    func load() -> Task<Void, Never>? {
        request(loader: loadEvidence, failureKey: "camera.error.read_failed")
    }

    @discardableResult
    func loadMedia() -> Task<Void, Never>? {
        request(loader: loadMediaEvidence, failureKey: "camera.error.media_list_failed")
    }

    private func request(loader: @escaping EvidenceLoader, failureKey: String) -> Task<Void, Never>? {
        isReading = true
        readErrorKey = nil
        prepareForEvidenceRefresh()
        readTask?.cancel()
        readGeneration &+= 1
        let generation = readGeneration
        let loadEvidence = loader
        let observePermissionRequired = self.observePermissionRequired
        let annotateCapture = self.annotateCapture
        let origin = self.origin
        readTask = Task { @MainActor in
            defer {
                if generation == readGeneration {
                    isReading = false
                    readTask = nil
                }
            }
            do {
                let evidence = try await loadEvidence(origin.address, origin.port)
                guard generation == readGeneration, !Task.isCancelled else { return }
                annotateCapture?("camera_profile", "novatek_r3_pro")
                annotateCapture?("camera_firmware", evidence.firmwareVersion)
            } catch CameraReadOnlyRequestError.pathUnavailable {
                if generation == readGeneration { readErrorKey = "camera.connection.detail.wifi_required" }
            } catch CameraReadOnlyRequestError.unsupportedProfile {
                if generation == readGeneration { readErrorKey = "camera.error.unsupported_profile" }
            } catch let error as URLError where error.code == .notConnectedToInternet {
                if generation == readGeneration {
                    observePermissionRequired()
                    readErrorKey = "camera.error.read_failed"
                }
            } catch {
                if generation == readGeneration, !Task.isCancelled {
                    readErrorKey = failureKey
                }
            }
        }
        return readTask
    }

    @discardableResult
    func connectIfNeeded(connection: CameraConnectionPresentation) -> Task<Void, Never>? {
        guard connection == .notConfigured else { return nil }
        return load()
    }

    func cancel() {
        readGeneration &+= 1
        readTask?.cancel()
        readTask = nil
        isReading = false
    }

    func clearError() {
        readErrorKey = nil
    }
}
