import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// Native task and presentation ownership. Rust retains the source, snapshot,
/// operation identity and every submission transition.
@MainActor
@Observable
final class CaptureSubmissionModel {
    typealias LoadStatus =
        @MainActor (MobileCaptureSubmissionSourceDto) async throws ->
        MobileCaptureSubmissionStatusDto?
    typealias Prepare = @MainActor (MobileCaptureSubmissionSourceDto) async throws -> MobileCaptureSubmissionStatusDto
    typealias Credential = @MainActor () async throws -> ChargingCommunityCredential
    typealias ExistingCredential = @MainActor () async throws -> ChargingCommunityCredential?
    typealias Send = @MainActor (String, ChargingCommunityCredential) async throws -> MobileCaptureSubmissionStatusDto
    typealias Progress = @MainActor @Sendable (MobileCaptureSubmissionProgressDto) -> Void
    typealias SendWithProgress =
        @MainActor (
            String, ChargingCommunityCredential, @escaping Progress
        ) async throws -> MobileCaptureSubmissionStatusDto
    typealias Cancel =
        @MainActor (String, ChargingCommunityCredential?) async throws ->
        MobileCaptureSubmissionStatusDto

    let source: MobileCaptureSubmissionSourceDto
    private(set) var status: MobileCaptureSubmissionStatusDto?
    private(set) var isWorking = false
    private(set) var isSending = false
    private(set) var progress: MobileCaptureSubmissionProgressDto?
    private(set) var isCancelling = false
    private(set) var error: String?
    @ObservationIgnored private let loadStatus: LoadStatus
    @ObservationIgnored private let prepareOperation: Prepare
    @ObservationIgnored private let credential: Credential
    @ObservationIgnored private let existingCredential: ExistingCredential
    @ObservationIgnored private let sendOperation: SendWithProgress
    @ObservationIgnored private let cancelOperation: Cancel
    @ObservationIgnored private let deleteOperation: Send
    @ObservationIgnored private var publicationGeneration: UInt64 = 0

    init(
        source: MobileCaptureSubmissionSourceDto,
        loadStatus: @escaping LoadStatus = readStatus,
        prepare: @escaping Prepare = prepareSnapshot,
        credential: @escaping Credential = { try await ChargingCommunitySync.shared.credentialForCaptureSubmission() },
        existingCredential: @escaping ExistingCredential = {
            try await ChargingCommunitySync.shared.existingCredentialForCaptureSubmission()
        },
        send: @escaping SendWithProgress = sendSnapshot,
        cancel: @escaping Cancel = cancelSubmission,
        delete: @escaping Send = deleteSubmission
    ) {
        self.source = source
        self.loadStatus = loadStatus
        prepareOperation = prepare
        self.credential = credential
        self.existingCredential = existingCredential
        sendOperation = send
        cancelOperation = cancel
        deleteOperation = delete
    }

    func refresh() async {
        guard !isWorking, !isSending else { return }
        await perform { try await self.loadStatus(self.source) }
    }

    func prepare() async {
        guard !isWorking, !isSending else { return }
        await perform { try await self.prepareOperation(self.source) }
    }

    /// The view calls this only from the user's Send confirmation.
    func send() async {
        guard !isWorking, !isSending, let operationID = status?.operationId else { return }
        isSending = true
        progress = nil
        defer {
            isSending = false
            progress = nil
        }
        await perform(readBackOnFailure: true) {
            let generation = self.publicationGeneration
            let credential = try await self.credential()
            return try await self.sendOperation(operationID, credential) { [weak self] progress in
                guard let self, self.isSending, generation == self.publicationGeneration else { return }
                // A cancelled attempt is replaced with a new operation id before upload.
                // Main-actor deliveries can be queued; never move the display backwards.
                if let previous = self.progress {
                    guard progress.operationId == previous.operationId,
                        progress.attemptId == previous.attemptId,
                        progress.bytesSent >= previous.bytesSent,
                        previous.phase != .verifyingReceipt || progress.phase == .verifyingReceipt
                    else { return }
                }
                self.progress = progress
            }
        }
    }

    func cancel() async {
        guard !isCancelling, !isWorking || isSending, let operationID = status?.operationId else { return }
        isCancelling = true
        progress = nil
        defer { isCancelling = false }
        // Cancellation can interrupt the native await for Send. Its Rust result
        // owns the new presentation; a late Send callback cannot replace it.
        await perform(readBackOnFailure: true) {
            let credential = try await self.existingCredential()
            return try await self.cancelOperation(operationID, credential)
        }
    }

    func delete() async {
        guard !isWorking, !isSending, let operationID = status?.operationId else { return }
        await perform(readBackOnFailure: true) {
            guard let credential = try await self.existingCredential() else {
                throw MobileChargingError.Unauthorized
            }
            return try await self.deleteOperation(operationID, credential)
        }
    }

    private func perform(
        readBackOnFailure: Bool = false,
        _ operation: @MainActor () async throws -> MobileCaptureSubmissionStatusDto?
    ) async {
        publicationGeneration &+= 1
        let generation = publicationGeneration
        isWorking = true
        error = nil
        do {
            let result = try await operation()
            guard generation == publicationGeneration else { return }
            status = result
        } catch {
            let message = Self.message(for: error)
            var latest: MobileCaptureSubmissionStatusDto?
            if readBackOnFailure { latest = try? await loadStatus(source) }
            guard generation == publicationGeneration else { return }
            if let latest { status = latest }
            self.error = message
        }
        if generation == publicationGeneration { isWorking = false }
    }

    private static func client() async throws -> CaptureSubmissionClientHandle {
        let database = try await RustPersistenceStore.open()
        return try CaptureSubmissionClientHandle(database: database)
    }

    static func message(for error: Error) -> String {
        if let error = error as? MobileCaptureSubmissionError {
            return captureSubmissionErrorMessage(error: error)
        }
        if error as? MobileChargingError == .Unauthorized || error as? MobileChargingError == .Unavailable {
            return captureSubmissionErrorMessage(error: .Unauthorized)
        }
        return "The private transfer could not be completed. Your original recording is still on this device."
    }

    var failureMessage: String? {
        status?.failure.map { captureSubmissionFailureMessage(failure: $0) }
    }

    private static func readStatus(_ source: MobileCaptureSubmissionSourceDto) async throws
        -> MobileCaptureSubmissionStatusDto?
    {
        try await client().status(source: source)
    }

    private static func prepareSnapshot(_ source: MobileCaptureSubmissionSourceDto) async throws
        -> MobileCaptureSubmissionStatusDto
    {
        try await client().prepare(source: source)
    }

    private static func sendSnapshot(
        _ operationID: String, _ credential: ChargingCommunityCredential, _ progress: @escaping Progress
    ) async throws -> MobileCaptureSubmissionStatusDto {
        try await client().sendWithProgress(
            operationId: operationID, accountId: credential.identityID, token: credential.token,
            listener: CaptureSendProgressListener(update: progress))
    }

    private static func cancelSubmission(_ operationID: String, _ credential: ChargingCommunityCredential?) async throws
        -> MobileCaptureSubmissionStatusDto
    {
        try await client().cancel(operationId: operationID, accountId: credential?.identityID, token: credential?.token)
    }

    private static func deleteSubmission(_ operationID: String, _ credential: ChargingCommunityCredential) async throws
        -> MobileCaptureSubmissionStatusDto
    {
        try await client().delete(operationId: operationID, accountId: credential.identityID, token: credential.token)
    }
}

/// UniFFI invokes the listener from the Rust transport executor.
private final class CaptureSendProgressListener: MobileCaptureSubmissionProgressListener, Sendable {
    let update: CaptureSubmissionModel.Progress

    init(update: @escaping CaptureSubmissionModel.Progress) { self.update = update }

    func onProgress(progress: MobileCaptureSubmissionProgressDto) {
        Task { @MainActor [update] in update(progress) }
    }
}
