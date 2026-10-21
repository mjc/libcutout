import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp

@MainActor
final class CaptureSubmissionModelTests: XCTestCase {
    private enum TestFailure: Error { case unavailable }

    private func status(_ phase: MobileCaptureSubmissionPhaseDto, operationID: String = "fixed-operation")
        -> MobileCaptureSubmissionStatusDto
    {
        MobileCaptureSubmissionStatusDto(
            operationId: operationID, phase: phase,
            categories: MobileCaptureSubmissionCategoriesDto(
                deviceIdentifiers: true, location: false, notes: true, annotations: true, settingsObservations: false
            ), droppedMessages: 2, byteCount: 100, receipt: nil, failure: nil
        )
    }

    func testRetryUsesThePreparedRustOperationAndNeverPreparesAReplacement() async {
        let prepared = status(.prepared)
        let failed = status(.failed)
        let sent = status(.sent)
        let credential = ChargingCommunityCredential(identityID: UUID().uuidString, token: "test-token")
        var operations: [String] = []
        var preparations = 0
        var credentials = 0
        let model = CaptureSubmissionModel(
            source: .liveCapture(id: UUID().uuidString),
            loadStatus: { _ in nil },
            prepare: { _ in
                preparations += 1
                return prepared
            },
            credential: {
                credentials += 1
                return credential
            },
            send: { operation, receivedCredential, _ in
                XCTAssertEqual(receivedCredential, credential)
                operations.append(operation)
                return operations.count == 1 ? failed : sent
            },
            cancel: { _, _ in throw TestFailure.unavailable },
            delete: { _, _ in throw TestFailure.unavailable }
        )

        await model.prepare()
        XCTAssertEqual(credentials, 0)
        await model.send()
        XCTAssertEqual(model.status?.phase, .failed)
        await model.send()
        XCTAssertEqual(model.status?.phase, .sent)
        XCTAssertEqual(operations, [prepared.operationId, prepared.operationId])
        XCTAssertEqual(preparations, 1)
        XCTAssertEqual(credentials, 2, "Each explicit Send action obtains its current account credential")
    }

    func testRelaunchDisplaysTheStoredStatusWithoutResumingSend() async {
        let stored = status(.sending)
        var sends = 0
        var credentials = 0
        let model = CaptureSubmissionModel(
            source: .liveCapture(id: UUID().uuidString),
            loadStatus: { _ in stored },
            prepare: { _ in throw TestFailure.unavailable },
            credential: {
                credentials += 1
                throw TestFailure.unavailable
            },
            send: { _, _, _ in
                sends += 1
                throw TestFailure.unavailable
            },
            cancel: { _, _ in throw TestFailure.unavailable },
            delete: { _, _ in throw TestFailure.unavailable }
        )
        await model.refresh()
        XCTAssertEqual(model.status, stored)
        XCTAssertNil(model.progress, "Relaunch does not imply that an upload resumes")
        XCTAssertEqual(sends, 0)
        XCTAssertEqual(credentials, 0)
    }

    func testCancelAndDeleteNeverRegisterAnAccount() async {
        let prepared = status(.prepared)
        let cancelled = status(.cancelled)
        var registrations = 0
        var cancellations = 0
        var deletions = 0
        let model = CaptureSubmissionModel(
            source: .liveCapture(id: UUID().uuidString),
            loadStatus: { _ in prepared },
            prepare: { _ in prepared },
            credential: {
                registrations += 1
                throw TestFailure.unavailable
            },
            existingCredential: { nil },
            send: { _, _, _ in throw TestFailure.unavailable },
            cancel: { operation, credential in
                XCTAssertEqual(operation, prepared.operationId)
                XCTAssertNil(credential)
                cancellations += 1
                return cancelled
            },
            delete: { _, _ in
                deletions += 1
                throw TestFailure.unavailable
            }
        )
        await model.refresh()
        await model.cancel()
        XCTAssertEqual(model.status?.phase, .cancelled)
        await model.delete()
        XCTAssertNotNil(model.error)
        XCTAssertEqual(registrations, 0)
        XCTAssertEqual(cancellations, 1)
        XCTAssertEqual(deletions, 0, "Missing credentials cannot claim a remote copy was deleted")
    }

    func testLateSendCompletionCannotReplaceTheCancellationPresentation() async {
        let prepared = status(.prepared)
        let cancelled = status(.cancelled)
        let lateSent = status(.sent)
        let credential = ChargingCommunityCredential(identityID: UUID().uuidString, token: "test-token")
        let entered = expectation(description: "Send is in flight")
        let gate = CaptureSubmissionSendGate(entered: entered)
        var sends = 0
        let source = MobileCaptureSubmissionSourceDto.liveCapture(id: UUID().uuidString)
        let model = CaptureSubmissionModel(
            source: source,
            loadStatus: { _ in prepared },
            prepare: { _ in prepared },
            credential: { credential },
            existingCredential: { credential },
            send: { _, _, _ in
                sends += 1
                await gate.suspend()
                return lateSent
            },
            cancel: { _, received in
                XCTAssertEqual(received, credential)
                return cancelled
            },
            delete: { _, _ in throw TestFailure.unavailable }
        )
        await model.refresh()
        let sending = Task { await model.send() }
        await fulfillment(of: [entered], timeout: 2)
        await model.send()
        XCTAssertEqual(sends, 1)
        await model.cancel()
        XCTAssertEqual(model.status?.phase, .cancelled)
        await gate.release()
        await sending.value
        XCTAssertEqual(model.status?.phase, .cancelled)
        XCTAssertEqual(model.source, source)
        XCTAssertFalse(model.isWorking)
        XCTAssertFalse(model.isSending)
    }

    func testOpeningRefreshingAndPreparingNeverAcquireCredentialsOrUpload() async {
        let source = MobileCaptureSubmissionSourceDto.liveCapture(id: UUID().uuidString)
        var reads = 0
        var preparations = 0
        var credentials = 0
        var sends = 0
        let model = CaptureSubmissionModel(
            source: source,
            loadStatus: { received in
                XCTAssertEqual(received, source)
                reads += 1
                return nil
            },
            prepare: { received in
                XCTAssertEqual(received, source)
                preparations += 1
                throw TestFailure.unavailable
            },
            credential: {
                credentials += 1
                throw TestFailure.unavailable
            },
            send: { _, _, _ in
                sends += 1
                throw TestFailure.unavailable
            },
            cancel: { _, _ in throw TestFailure.unavailable },
            delete: { _, _ in throw TestFailure.unavailable }
        )

        XCTAssertEqual(reads, 0)
        await model.refresh()
        await model.refresh()
        await model.prepare()
        XCTAssertEqual(reads, 2)
        XCTAssertEqual(preparations, 1)
        XCTAssertEqual(credentials, 0, "Consent preparation is local and must not register an identity")
        XCTAssertEqual(sends, 0, "Only the confirmed Send action may upload")
    }

    func testUploadProgressWaitsForTheDurableReceipt() async {
        let prepared = status(.prepared)
        let credential = ChargingCommunityCredential(identityID: UUID().uuidString, token: "test-token")
        let uploading = expectation(description: "Partial upload")
        let verifying = expectation(description: "Body uploaded; receipt pending")
        let uploadGate = CaptureSubmissionSendGate(entered: uploading)
        let receiptGate = CaptureSubmissionSendGate(entered: verifying)
        let sent = status(.sent)
        let model = CaptureSubmissionModel(
            source: .liveCapture(id: UUID().uuidString), loadStatus: { _ in prepared },
            credential: { credential },
            send: { operation, _, progress in
                progress(.init(operationId: operation, attemptId: 1, bytesSent: 25, totalBytes: 100, phase: .uploading))
                await uploadGate.suspend()
                progress(
                    .init(
                        operationId: operation, attemptId: 1, bytesSent: 100, totalBytes: 100, phase: .verifyingReceipt)
                )
                await receiptGate.suspend()
                return sent
            })
        await model.refresh()
        let sending = Task { await model.send() }
        await fulfillment(of: [uploading], timeout: 2)
        XCTAssertEqual(model.progress?.bytesSent, 25)
        XCTAssertEqual(model.progress?.phase, .uploading)
        XCTAssertNotEqual(model.status?.phase, .sent)
        await uploadGate.release()
        await fulfillment(of: [verifying], timeout: 2)
        XCTAssertEqual(model.progress?.bytesSent, 100)
        XCTAssertEqual(model.progress?.phase, .verifyingReceipt)
        XCTAssertNotEqual(model.status?.phase, .sent, "An uploaded body is not a durable receipt")
        await receiptGate.release()
        await sending.value
        XCTAssertEqual(model.status?.phase, .sent)
        XCTAssertNil(model.progress)
    }

    func testRetryAndCancellationDiscardOldProgress() async {
        let prepared = status(.prepared)
        let failed = status(.failed)
        let cancelled = status(.cancelled)
        let credential = ChargingCommunityCredential(identityID: UUID().uuidString, token: "test-token")
        let entered = expectation(description: "Retry uploading")
        let gate = CaptureSubmissionSendGate(entered: entered)
        var callbacks: [CaptureSubmissionModel.Progress] = []
        let model = CaptureSubmissionModel(
            source: .liveCapture(id: UUID().uuidString), loadStatus: { _ in prepared },
            credential: { credential }, existingCredential: { credential },
            send: { operation, _, progress in
                callbacks.append(progress)
                XCTAssertEqual(operation, prepared.operationId)
                if callbacks.count == 1 {
                    progress(
                        .init(operationId: operation, attemptId: 1, bytesSent: 75, totalBytes: 100, phase: .uploading))
                    return failed
                }
                progress(.init(operationId: operation, attemptId: 2, bytesSent: 0, totalBytes: 100, phase: .uploading))
                await gate.suspend()
                return failed
            }, cancel: { _, _ in cancelled })
        await model.refresh()
        await model.send()
        XCTAssertNil(model.progress, "Failure must clear the old byte count")
        let retry = Task { await model.send() }
        await fulfillment(of: [entered], timeout: 2)
        XCTAssertEqual(model.progress?.bytesSent, 0, "Retry uploads the snapshot from the beginning")
        callbacks[0](
            .init(operationId: prepared.operationId, attemptId: 1, bytesSent: 99, totalBytes: 100, phase: .uploading))
        callbacks[1](
            .init(operationId: "another-operation", attemptId: 2, bytesSent: 99, totalBytes: 100, phase: .uploading))
        XCTAssertEqual(model.progress?.bytesSent, 0)
        callbacks[1](
            .init(operationId: prepared.operationId, attemptId: 2, bytesSent: 50, totalBytes: 100, phase: .uploading))
        callbacks[1](
            .init(operationId: prepared.operationId, attemptId: 2, bytesSent: 25, totalBytes: 100, phase: .uploading))
        XCTAssertEqual(model.progress?.bytesSent, 50, "Queued callbacks cannot regress progress")
        await model.cancel()
        XCTAssertNil(model.progress)
        callbacks[1](
            .init(
                operationId: prepared.operationId, attemptId: 2, bytesSent: 100, totalBytes: 100,
                phase: .verifyingReceipt))
        XCTAssertNil(model.progress)
        await gate.release()
        await retry.value
        XCTAssertEqual(model.status?.phase, .cancelled)
        XCTAssertNil(model.progress)
    }

    func testSharingAnExportHasNoSubmissionOrCredentialEffect() async {
        var exports = 0
        let source = URL(fileURLWithPath: "/tmp/local-capture.jsonl")
        let shared = URL(fileURLWithPath: "/tmp/share-capture.jsonl")
        let export = CaptureExportModel(sourceExport: { received in
            XCTAssertEqual(received, source)
            exports += 1
            return shared
        })

        await export.export(source: source)
        XCTAssertEqual(export.fileURL, shared)
        XCTAssertEqual(exports, 1)
    }
}

private actor CaptureSubmissionSendGate {
    let entered: XCTestExpectation
    private var continuation: CheckedContinuation<Void, Never>?

    init(entered: XCTestExpectation) { self.entered = entered }

    func suspend() async {
        await withCheckedContinuation {
            continuation = $0
            entered.fulfill()
        }
    }

    func release() {
        continuation?.resume()
        continuation = nil
    }
}
