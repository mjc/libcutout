import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

@MainActor
@Observable
final class CameraMediaModel {
    private(set) var downloadedMediaURL: URL?
    private(set) var downloadedMediaPath: String?
    private(set) var downloadingMediaPath: String?
    private(set) var mediaErrorKey: String?
    private(set) var thumbnailDataByPath: [String: Data] = [:]
    private(set) var thumbnailPathInFlight: String?
    private(set) var thumbnailErrorKey: String?

    @ObservationIgnored private let adapter: CameraLocalNetworkAdapter
    @ObservationIgnored private let recordMediaReference:
        ((String, CaptureGeneration, MobileCameraMediaProvenanceDto, URL) -> Void)?
    @ObservationIgnored private let currentCaptureIdentity: (() -> (fileName: String, generation: CaptureGeneration)?)?
    @ObservationIgnored private var mediaDownloadTask: Task<Void, Never>?
    @ObservationIgnored private var thumbnailTask: Task<Void, Never>?
    @ObservationIgnored private var mediaDownloadGeneration: UInt64 = 0
    @ObservationIgnored private var thumbnailGeneration: UInt64 = 0

    private static let maximumThumbnailEntries = 32

    init(
        adapter: CameraLocalNetworkAdapter = CameraLocalNetworkAdapter(),
        recordMediaReference:
            ((String, CaptureGeneration, MobileCameraMediaProvenanceDto, URL) -> Void)? = nil,
        currentCaptureIdentity: (() -> (fileName: String, generation: CaptureGeneration)?)? = nil
    ) {
        self.adapter = adapter
        self.recordMediaReference = recordMediaReference
        self.currentCaptureIdentity = currentCaptureIdentity
    }

    func prepareForEvidenceRefresh() {
        thumbnailGeneration &+= 1
        thumbnailTask?.cancel()
        thumbnailTask = nil
        thumbnailPathInFlight = nil
        thumbnailDataByPath.removeAll(keepingCapacity: true)
    }

    func downloadMedia(_ media: CameraMediaEvidence, address: String, port: String) {
        guard let portNumber = UInt16(port) else {
            mediaErrorKey = "camera.error.invalid_port"
            return
        }

        let destination = mediaOutputURL(for: media)
        let captureIdentity = currentCaptureIdentity?()
        mediaDownloadTask?.cancel()
        mediaDownloadGeneration &+= 1
        let generation = mediaDownloadGeneration
        downloadingMediaPath = media.path
        downloadedMediaURL = nil
        downloadedMediaPath = nil
        mediaErrorKey = nil
        mediaDownloadTask = Task { @MainActor in
            defer {
                if generation == mediaDownloadGeneration {
                    downloadingMediaPath = nil
                    mediaDownloadTask = nil
                }
            }
            do {
                let association = captureIdentity.map {
                    MobileNovatekMediaDownloadAssociationInput(
                        captureGeneration: $0.generation.dto,
                        rideCaptureFileName: $0.fileName,
                        capturedAtMonotonicMs: DispatchTime.now().uptimeNanoseconds / 1_000_000,
                        capturedAtWallClockMs: UInt64(max(0, Date().timeIntervalSince1970 * 1_000)),
                        clockUncertainty: .unknown
                    )
                }
                let result = try await adapter.downloadMedia(
                    address: address,
                    port: portNumber,
                    media: media,
                    to: destination,
                    association: association
                )
                guard generation == mediaDownloadGeneration else { return }
                downloadedMediaURL = destination
                downloadedMediaPath = result.media.path
                if let captureIdentity, let provenance = result.provenance {
                    recordMediaReference?(
                        captureIdentity.fileName,
                        captureIdentity.generation,
                        provenance,
                        destination
                    )
                }
            } catch is CancellationError {
                // Cancellation is an expected user action, not a transfer error.
            } catch let error as URLError where error.code == .cancelled {
                // URLSession reports cancellation as URLError on some OSes.
            } catch CameraMediaDownloadError.originMismatch {
                if generation == mediaDownloadGeneration {
                    mediaErrorKey = "camera.error.origin_mismatch"
                }
            } catch {
                if generation == mediaDownloadGeneration {
                    mediaErrorKey = "camera.error.media_download_failed"
                }
            }
        }
    }

    func cancelMediaDownload() {
        mediaDownloadGeneration &+= 1
        mediaDownloadTask?.cancel()
        mediaDownloadTask = nil
        downloadingMediaPath = nil
    }

    func fetchThumbnail(_ media: CameraMediaEvidence, address: String, port: String) {
        guard let portNumber = UInt16(port) else {
            thumbnailErrorKey = "camera.error.invalid_port"
            return
        }

        thumbnailGeneration &+= 1
        let generation = thumbnailGeneration
        thumbnailTask?.cancel()
        thumbnailPathInFlight = media.path
        thumbnailErrorKey = nil
        thumbnailTask = Task { @MainActor in
            defer {
                if generation == thumbnailGeneration {
                    thumbnailPathInFlight = nil
                    thumbnailTask = nil
                }
            }
            do {
                let data = try await adapter.fetchMediaThumbnail(
                    address: address,
                    port: portNumber,
                    media: media
                )
                guard generation == thumbnailGeneration else { return }
                guard !data.isEmpty else {
                    thumbnailErrorKey = "camera.error.thumbnail_empty"
                    return
                }
                if thumbnailDataByPath.count >= Self.maximumThumbnailEntries,
                    thumbnailDataByPath[media.path] == nil,
                    let oldestPath = thumbnailDataByPath.keys.first
                {
                    thumbnailDataByPath.removeValue(forKey: oldestPath)
                }
                thumbnailDataByPath[media.path] = data
            } catch is CancellationError {
                // Cancellation is an expected user action.
            } catch CameraReadOnlyRequestError.originMismatch {
                if generation == thumbnailGeneration {
                    thumbnailErrorKey = "camera.error.origin_mismatch"
                }
            } catch {
                if generation == thumbnailGeneration {
                    thumbnailErrorKey = "camera.error.thumbnail_failed"
                }
            }
        }
    }

    func cancelOutstandingWork() {
        mediaDownloadGeneration &+= 1
        thumbnailGeneration &+= 1
        mediaDownloadTask?.cancel()
        thumbnailTask?.cancel()
        mediaDownloadTask = nil
        thumbnailTask = nil
        downloadingMediaPath = nil
        thumbnailPathInFlight = nil
    }

    private func mediaOutputURL(for media: CameraMediaEvidence) -> URL {
        let directory =
            FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return directory.appendingPathComponent(
            "camera-media-" + UUID().uuidString + "-" + cameraMediaLocalFileComponent(media.name)
        )
    }
}
