import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// Native task and presentation ownership. Rust retains the source, snapshot,
/// operation identity and every submission transition.
@MainActor
@Observable
final class CaptureSubmissionModel {
    typealias LoadStatus = @MainActor (MobileCaptureSubmissionSourceDto) async throws ->
        MobileCaptureSubmissionStatusDto?
    typealias Prepare = @MainActor (MobileCaptureSubmissionSourceDto) async throws -> MobileCaptureSubmissionStatusDto
    typealias Credential = @MainActor () async throws -> ChargingCommunityCredential
    typealias ExistingCredential = @MainActor () async throws -> ChargingCommunityCredential?
    typealias Send = @MainActor (String, ChargingCommunityCredential) async throws -> MobileCaptureSubmissionStatusDto
    typealias Cancel = @MainActor (String, ChargingCommunityCredential?) async throws ->
        MobileCaptureSubmissionStatusDto

    let source: MobileCaptureSubmissionSourceDto
    private(set) var status: MobileCaptureSubmissionStatusDto?
    private(set) var isWorking = false
    private(set) var isSending = false
    private(set) var isCancelling = false
    private(set) var error: String?
    @ObservationIgnored private let loadStatus: LoadStatus
    @ObservationIgnored private let prepareOperation: Prepare
    @ObservationIgnored private let credential: Credential
    @ObservationIgnored private let existingCredential: ExistingCredential
    @ObservationIgnored private let sendOperation: Send
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
        send: @escaping Send = sendSnapshot,
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
        defer { isSending = false }
        await perform(readBackOnFailure: true) {
            let credential = try await self.credential()
            return try await self.sendOperation(operationID, credential)
        }
    }

    func cancel() async {
        guard !isCancelling, !isWorking || isSending, let operationID = status?.operationId else { return }
        isCancelling = true
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

    private static func message(for error: Error) -> String {
        guard let error = error as? MobileCaptureSubmissionError else {
            if error as? MobileChargingError == .Unauthorized || error as? MobileChargingError == .Unavailable {
                return "The account cannot authorize this transfer. A remote copy has not been confirmed removed."
            }
            return "The private transfer could not be completed. Your original recording is still on this device."
        }
        switch error {
        case .InvalidSource: return "This recording has no verified source for private Send."
        case .NotReady: return "This recording is not ready for private Send."
        case .Unavailable: return "Private Send is unavailable. Try again later."
        case .Unauthorized:
            return "The account cannot authorize this transfer. A remote copy has not been confirmed removed."
        case .RateLimited: return "Too many requests. Try again later."
        case .QuotaExceeded: return "Private capture storage is full. Try again later."
        case .InvalidCapture: return "The recording could not be validated for private Send."
        case .StorageFailure:
            return "The transfer status could not be saved. Your original recording is still on this device."
        case .Cancelled: return "The transfer was cancelled."
        case .ReceiptMismatch:
            return "The server receipt did not match this snapshot. Receipt confirmation is still required."
        }
    }

    var failureMessage: String? {
        switch status?.failure {
        case .network: "The network request failed. Retry sends the same saved snapshot."
        case .unauthorized: "The account cannot authorize this transfer."
        case .forbidden: "This account cannot access the private transfer."
        case .conflict:
            "The server rejected conflicting submission information. Your original recording is still on this device."
        case .rejected: "The service rejected this capture."
        case .throttled: "Too many requests. Try again later."
        case .unavailable: "Private Send is unavailable. Try again later."
        case .tooLarge: "This capture exceeds the service's size limit."
        case .invalidCapture: "The service could not validate this capture."
        case .musicPresent: "This snapshot contains excluded music metadata and cannot be sent."
        case .quotaExceeded: "Private capture storage is full. Try again later."
        case .cancelled: "The transfer was cancelled."
        case nil: nil
        }
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

    private static func sendSnapshot(_ operationID: String, _ credential: ChargingCommunityCredential) async throws
        -> MobileCaptureSubmissionStatusDto
    {
        try await client().send(operationId: operationID, accountId: credential.identityID, token: credential.token)
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
