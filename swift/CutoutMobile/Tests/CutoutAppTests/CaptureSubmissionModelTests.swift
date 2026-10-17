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
                deviceIdentifiers: true, location: false, notes: true, annotations: true
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
            send: { operation, receivedCredential in
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
            send: { _, _ in
                sends += 1
                throw TestFailure.unavailable
            },
            cancel: { _, _ in throw TestFailure.unavailable },
            delete: { _, _ in throw TestFailure.unavailable }
        )
        await model.refresh()
        XCTAssertEqual(model.status, stored)
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
            send: { _, _ in throw TestFailure.unavailable },
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
            send: { _, _ in
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
            send: { _, _ in
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
