import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp

@MainActor
final class CaptureSubmissionModelTests: XCTestCase {
    private enum TestFailure: Error { case unavailable }

    func testEvidenceLoadErrorKeepsReviewDisabledUntilRetrySucceeds() async {
        let source = MobileCaptureSubmissionSourceDto.liveCapture(id: UUID().uuidString)
        var attempts = 0
        let model = CaptureEvidenceEditorModel(source: source) { received in
            XCTAssertEqual(received, source)
            attempts += 1
            if attempts == 1 { throw TestFailure.unavailable }
            return MobileCaptureEvidenceDto(
                note: "", screenshots: [], noteLimitBytes: 1_024,
                screenshotLimit: 4, screenshotLimitBytes: 5 * 1_024 * 1_024
            )
        }

        await model.refresh()
        XCTAssertFalse(model.isLoaded)
        XCTAssertNotNil(model.error)
        XCTAssertFalse(
            captureSubmissionEvidenceIsReviewable(
                loaded: model.isLoaded, working: model.isWorking, dirty: model.isDirty))

        await model.refresh()
        XCTAssertTrue(model.isLoaded)
        XCTAssertNil(model.error)
        XCTAssertTrue(
            captureSubmissionEvidenceIsReviewable(
                loaded: model.isLoaded, working: model.isWorking, dirty: model.isDirty))
        XCTAssertEqual(attempts, 2)
    }

    private func status(
        _ phase: MobileCaptureSubmissionPhaseDto,
        operationID: String = "fixed-operation",
        receipt: MobileCaptureSubmissionReceiptDto? = nil,
        localPayloadState: MobileCaptureSubmissionLocalPayloadStateDto = .retained
    )
        -> MobileCaptureSubmissionStatusDto
    {
        MobileCaptureSubmissionStatusDto(
            operationId: operationID, phase: phase, origin: .unknown, integrity: .unknown,
            localPayloadState: localPayloadState,
            categories: MobileCaptureSubmissionCategoriesDto(
                deviceIdentifiers: true, location: false, notes: true, annotations: true, settingsObservations: false
            ), droppedMessages: 2, byteCount: 100, receipt: receipt, failure: nil
        )
    }

    func testHistoryBadgeRequiresDurableSendReceiptAndReportsLocalRemoval() {
        XCTAssertNil(captureHistorySendBadge(status(.sent)))
        let receipt = MobileCaptureSubmissionReceiptDto(
            operationId: "fixed-operation", sha256: String(repeating: "a", count: 64),
            bytes: 100, receivedAtMilliseconds: 1_700_000_000_000
        )
        XCTAssertEqual(captureHistorySendBadge(status(.sent, receipt: receipt)), .sent)
        XCTAssertEqual(
            captureHistorySendBadge(status(.sent, receipt: receipt, localPayloadState: .removed)),
            .localDeleted)
        XCTAssertEqual(captureHistorySendBadge(status(.deleted, receipt: receipt)), .sent)
        XCTAssertEqual(
            captureHistorySendBadge(status(.deleted, receipt: receipt, localPayloadState: .removed)),
            .localDeleted)
        XCTAssertEqual(
            captureHistorySendBadge(status(.sent, receipt: receipt, localPayloadState: .removalPending)),
            .sent)
        XCTAssertEqual(captureHistorySendBadge(status(.deleting, receipt: receipt)), .sent)
        XCTAssertNil(captureHistorySendBadge(status(.prepared, receipt: receipt)))
    }

    func testBatchReviewIsLocalAndConfirmationDelegatesOneSharedSend() async {
        let candidate = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "source-a", sourceKind: "Live capture", displayName: "Capture A",
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown", integrity: "Unknown",
            sourceBytes: 120, sourceSHA256: String(repeating: "a", count: 64),
            transferBytes: 240, transferSHA256: String(repeating: "b", count: 64),
            categories: ["device identifiers"], noteBytes: 4, screenshotCount: 1, receipt: nil
        )
        var sends: [[String]] = []
        var stopRequests = 0
        var terminalUpdates = 0
        let model = CaptureSubmissionBatchModel(
            adapter: CaptureSubmissionBatchAdapter(
                review: { _ in [candidate] },
                sendSelected: { ids, update in
                    sends.append(ids)
                    update(
                        CaptureSubmissionBatchProgress(
                            sourceArtifactID: candidate.sourceArtifactID, phase: .cleaned,
                            message: "Local copy removed after receipt", total: 1, sent: 1,
                            cleaned: 1, failed: 0, skipped: 0, cancelled: 0, current: 0
                        ))
                    return CaptureSubmissionBatchSummary(
                        sent: 1, cleaned: 1, failed: 0, skipped: 0, cancelled: 0,
                        failedSourceArtifactIDs: [],
                        pendingCleanupSourceArtifactIDs: []
                    )
                },
                requestCancel: { stopRequests += 1 }
            ),
            onTerminalUpdate: { terminalUpdates += 1 }
        )

        XCTAssertTrue(sends.isEmpty, "Creating or reviewing the plan must not send captures")
        await model.review()
        XCTAssertEqual(model.selectedCount, 1)
        XCTAssertTrue(sends.isEmpty, "The plan review remains local")
        await model.sendSelected()

        XCTAssertEqual(sends, [[candidate.sourceArtifactID]])
        XCTAssertEqual(model.itemProgress[candidate.sourceArtifactID]?.message, "Local copy removed after receipt")
        XCTAssertEqual(model.summary?.cleaned, 1)
        XCTAssertEqual(terminalUpdates, 1)
        model.requestStopAfterCurrent()
        XCTAssertEqual(stopRequests, 0, "Cancellation is available only while a batch is active")
    }

    func testBatchRetryIgnoresLateProgressFromPreviousAttempt() async {
        let candidate = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "source-a", sourceKind: "Live capture", displayName: nil,
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown", integrity: "Unknown",
            sourceBytes: 120, sourceSHA256: String(repeating: "a", count: 64),
            transferBytes: 240, transferSHA256: String(repeating: "b", count: 64),
            categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        var attempt = 0
        var reviews = 0
        var previousUpdate: CaptureSubmissionBatchAdapter.ProgressUpdate?
        let model = CaptureSubmissionBatchModel(
            adapter: CaptureSubmissionBatchAdapter(
                review: { _ in
                    reviews += 1
                    return [candidate]
                },
                sendSelected: { _, update in
                    attempt += 1
                    if attempt == 1 {
                        previousUpdate = update
                        return .init(
                            sent: 0, cleaned: 0, failed: 1, skipped: 0, cancelled: 0,
                            failedSourceArtifactIDs: [candidate.sourceArtifactID], pendingCleanupSourceArtifactIDs: [])
                    }

                    update(
                        .init(
                            sourceArtifactID: candidate.sourceArtifactID, phase: .sent, message: "receipt",
                            total: 1, sent: 1, cleaned: 0, failed: 0, skipped: 0, cancelled: 0, current: 0
                        ))
                    previousUpdate?(
                        .init(
                            sourceArtifactID: candidate.sourceArtifactID, phase: .failed, message: "stale failure",
                            total: 1, sent: 0, cleaned: 0, failed: 1, skipped: 0, cancelled: 0, current: 0
                        ))
                    return .init(
                        sent: 2, cleaned: 1, failed: 0, skipped: 0, cancelled: 0,
                        failedSourceArtifactIDs: [], pendingCleanupSourceArtifactIDs: [])
                },
                requestCancel: {}
            ))

        await model.review()
        await model.sendSelected()
        XCTAssertEqual(model.failedSourceArtifactIDs, [candidate.sourceArtifactID])
        await model.reviewFailed()
        XCTAssertEqual(reviews, 2)
        XCTAssertEqual(model.selectedSourceArtifactIDs, [candidate.sourceArtifactID])
        XCTAssertEqual(attempt, 1, "Refreshing the reviewed plan cannot send")
        await model.sendSelected()

        XCTAssertEqual(model.progress?.phase, .sent)
        XCTAssertEqual(model.summary?.sent, 2, "The final Rust summary replaces an older progress snapshot")
        XCTAssertEqual(model.itemProgress[candidate.sourceArtifactID]?.message, "receipt")
        XCTAssertTrue(model.failedSourceArtifactIDs.isEmpty)
    }

    func testStoppedBatchRequiresFreshLocalReviewBeforeAnotherSend() async {
        let candidate = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "source-stop", sourceKind: "Live capture", displayName: nil,
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown", integrity: "Unknown",
            sourceBytes: 120, sourceSHA256: String(repeating: "a", count: 64),
            transferBytes: 240, transferSHA256: String(repeating: "b", count: 64),
            categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        var reviews = 0
        var sends = 0
        var model: CaptureSubmissionBatchModel?
        let adapter = CaptureSubmissionBatchAdapter(
            review: { _ in
                reviews += 1
                return [candidate]
            },
            sendSelected: { _, update in
                sends += 1
                update(
                    .init(
                        sourceArtifactID: candidate.sourceArtifactID, phase: .current, message: "current",
                        total: 1, sent: 0, cleaned: 0, failed: 0, skipped: 0, cancelled: 0, current: 1
                    ))
                model?.requestStopAfterCurrent()
                return .init(
                    sent: 0, cleaned: 0, failed: 0, skipped: 0, cancelled: 1,
                    failedSourceArtifactIDs: [], pendingCleanupSourceArtifactIDs: [])
            },
            requestCancel: {}
        )
        model = CaptureSubmissionBatchModel(adapter: adapter)
        await model?.review()
        await model?.sendSelected()

        XCTAssertTrue(model?.requiresFreshReview == true)
        XCTAssertEqual(sends, 1)
        await model?.sendSelected()
        XCTAssertEqual(sends, 1, "The permanently cancelled plan cannot admit another batch")
        await model?.review()
        XCTAssertFalse(model?.requiresFreshReview ?? true)
        XCTAssertEqual(reviews, 2)
        XCTAssertEqual(sends, 1, "A fresh local review still does not send before confirmation")
    }

    func testReviewFailureUsesTheReviewMessageInsteadOfTheRawStorageCase() async {
        struct StorageCase: Error, LocalizedError {
            var errorDescription: String? { "cutoutmobileffi.mobilecapturesubmission.storagefailure" }
        }
        let model = CaptureSubmissionBatchModel(
            adapter: CaptureSubmissionBatchAdapter(
                review: { _ in throw StorageCase() },
                sendSelected: { _, _ in
                    CaptureSubmissionBatchSummary(
                        sent: 0, cleaned: 0, failed: 0, skipped: 0, cancelled: 0,
                        failedSourceArtifactIDs: [], pendingCleanupSourceArtifactIDs: [])
                },
                requestCancel: {}
            ))
        await model.review()
        XCTAssertEqual(model.error, localizedAppText("capture.batch.review_error"))
        XCTAssertFalse(model.error?.localizedCaseInsensitiveContains("storagefailure") ?? false)
        XCTAssertTrue(model.requiresFreshReview)
    }

    @MainActor
    func testReviewListsACaptureWhileItsPayloadIsStillBeingChecked() async {
        let checking = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "source-check", sourceKind: "Live capture", displayName: "Capture",
            isEligible: false, isChecking: true, skipReason: "Checking", origin: "Unknown",
            integrity: "Unknown", sourceBytes: 120, sourceSHA256: nil, transferBytes: nil,
            transferSHA256: nil, categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        let eligible = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "source-check", sourceKind: "Live capture", displayName: "Capture",
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown",
            integrity: "Unknown", sourceBytes: 120, sourceSHA256: String(repeating: "a", count: 64),
            transferBytes: 240, transferSHA256: String(repeating: "b", count: 64),
            categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        let release = AsyncStream<Void>.makeStream()
        let model = CaptureSubmissionBatchModel(
            adapter: CaptureSubmissionBatchAdapter(
                review: { onCandidate in
                    onCandidate(checking)
                    for await _ in release.stream { break }
                    onCandidate(eligible)
                    return [eligible]
                },
                sendSelected: { _, _ in
                    CaptureSubmissionBatchSummary(
                        sent: 0, cleaned: 0, failed: 0, skipped: 0, cancelled: 0,
                        failedSourceArtifactIDs: [], pendingCleanupSourceArtifactIDs: [])
                },
                requestCancel: {}
            ))
        let task = Task { await model.review() }
        var sawChecking = false
        for _ in 0..<40 {
            if model.candidates.contains(where: \.isChecking) {
                sawChecking = true
                break
            }
            await Task.yield()
        }
        XCTAssertTrue(sawChecking)
        XCTAssertTrue(model.isReviewing)
        XCTAssertTrue(model.requiresFreshReview)
        XCTAssertEqual(model.selectedCount, 0)
        release.continuation.yield(())
        release.continuation.finish()
        await task.value
        XCTAssertFalse(model.isReviewing)
        XCTAssertEqual(model.candidates.first?.isEligible, true)
        XCTAssertFalse(model.requiresFreshReview)
        XCTAssertEqual(model.selectedCount, 1)
        XCTAssertNil(model.error)
        XCTAssertTrue(model.canSend)
    }

    @MainActor
    func testSendAllIncludesCapturesCheckedAfterTheButton() async {
        let ready = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "ready", sourceKind: "Live capture", displayName: "Ready",
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown",
            integrity: "Unknown", sourceBytes: 120, sourceSHA256: String(repeating: "a", count: 64),
            transferBytes: 240, transferSHA256: String(repeating: "b", count: 64),
            categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        let checking = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "later", sourceKind: "Live capture", displayName: "Later",
            isEligible: false, isChecking: true, skipReason: "Checking", origin: "Unknown",
            integrity: "Unknown", sourceBytes: 80, sourceSHA256: nil, transferBytes: nil,
            transferSHA256: nil, categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        let later = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "later", sourceKind: "Live capture", displayName: "Later",
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown",
            integrity: "Unknown", sourceBytes: 80, sourceSHA256: String(repeating: "c", count: 64),
            transferBytes: 90, transferSHA256: String(repeating: "d", count: 64),
            categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        let release = AsyncStream<Void>.makeStream()
        final class Gate {
            var commits = 0
            var cancels = 0
        }
        let gate = Gate()
        var sent: [String] = []
        let model = CaptureSubmissionBatchModel(
            adapter: CaptureSubmissionBatchAdapter(
                review: { onCandidate in
                    onCandidate(ready)
                    onCandidate(checking)
                    for await _ in release.stream { break }
                    onCandidate(later)
                    return [ready, later]
                },
                sendSelected: { ids, _ in
                    sent = ids
                    return CaptureSubmissionBatchSummary(
                        sent: UInt64(ids.count), cleaned: 0, failed: 0, skipped: 0, cancelled: 0,
                        failedSourceArtifactIDs: [], pendingCleanupSourceArtifactIDs: [])
                },
                requestCancel: { gate.cancels += 1 },
                commitSendAll: { _ in
                    gate.commits += 1
                    release.continuation.yield(())
                    release.continuation.finish()
                    return true
                },
                takeCommittedSummary: {
                    CaptureSubmissionBatchSummary(
                        sent: 2, cleaned: 2, failed: 0, skipped: 0, cancelled: 0,
                        failedSourceArtifactIDs: [], pendingCleanupSourceArtifactIDs: [])
                }
            ))
        let task = Task { await model.review() }
        for _ in 0..<40 {
            if model.canSend { break }
            await Task.yield()
        }
        XCTAssertTrue(model.isReviewing)
        XCTAssertTrue(model.canSend)
        XCTAssertEqual(model.selectedCount, 1)
        await model.sendSelected()
        await task.value
        XCTAssertEqual(gate.commits, 1)
        XCTAssertEqual(gate.cancels, 0)
        XCTAssertTrue(sent.isEmpty)
        XCTAssertEqual(model.summary?.sent, 2)
        XCTAssertFalse(model.requiresFreshReview)
        XCTAssertTrue(model.didFinishSend)
        await model.sendSelected()
        XCTAssertTrue(sent.isEmpty)
    }

    @MainActor
    func testNarrowedSelectionStopsTheScanAndSendsOnlyThatCapture() async {
        let first = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "first", sourceKind: "Live capture", displayName: "First",
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown",
            integrity: "Unknown", sourceBytes: 120, sourceSHA256: String(repeating: "a", count: 64),
            transferBytes: 240, transferSHA256: String(repeating: "b", count: 64),
            categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        let second = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "second", sourceKind: "Live capture", displayName: "Second",
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown",
            integrity: "Unknown", sourceBytes: 80, sourceSHA256: String(repeating: "c", count: 64),
            transferBytes: 90, transferSHA256: String(repeating: "d", count: 64),
            categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        let release = AsyncStream<Void>.makeStream()
        var cancels = 0
        var commits = 0
        var sent: [String] = []
        let model = CaptureSubmissionBatchModel(
            adapter: CaptureSubmissionBatchAdapter(
                review: { onCandidate in
                    onCandidate(first)
                    onCandidate(second)
                    for await _ in release.stream { break }
                    return [first, second]
                },
                sendSelected: { ids, _ in
                    sent = ids
                    return CaptureSubmissionBatchSummary(
                        sent: UInt64(ids.count), cleaned: 0, failed: 0, skipped: 0, cancelled: 0,
                        failedSourceArtifactIDs: [], pendingCleanupSourceArtifactIDs: [])
                },
                requestCancel: {
                    cancels += 1
                    release.continuation.yield(())
                    release.continuation.finish()
                },
                commitSendAll: { _ in
                    commits += 1
                    return true
                }
            ))
        let task = Task { await model.review() }
        for _ in 0..<40 {
            if model.selectedCount == 2 { break }
            await Task.yield()
        }
        model.setSelected(false, candidate: second)
        await model.sendSelected()
        await task.value
        XCTAssertEqual(commits, 0)
        XCTAssertEqual(cancels, 1)
        XCTAssertEqual(sent, ["first"])
    }

    @MainActor
    func testClosingTheReviewKeepsTheCapturesAlreadySelected() async {
        let ready = CaptureSubmissionBatchCandidate(
            sourceArtifactID: "ready", sourceKind: "Live capture", displayName: "Ready",
            isEligible: true, isChecking: false, skipReason: nil, origin: "Unknown",
            integrity: "Unknown", sourceBytes: 120, sourceSHA256: String(repeating: "a", count: 64),
            transferBytes: 240, transferSHA256: String(repeating: "b", count: 64),
            categories: [], noteBytes: 0, screenshotCount: 0, receipt: nil
        )
        let release = AsyncStream<Void>.makeStream()
        var cancelRequests = 0
        let model = CaptureSubmissionBatchModel(
            adapter: CaptureSubmissionBatchAdapter(
                review: { onCandidate in
                    onCandidate(ready)
                    for await _ in release.stream { break }
                    return [ready]
                },
                sendSelected: { _, _ in
                    CaptureSubmissionBatchSummary(
                        sent: 0, cleaned: 0, failed: 0, skipped: 0, cancelled: 0,
                        failedSourceArtifactIDs: [], pendingCleanupSourceArtifactIDs: [])
                },
                requestCancel: { cancelRequests += 1 }
            ))
        model.ensureReviewStarted()
        for _ in 0..<40 {
            if model.canSend { break }
            await Task.yield()
        }
        XCTAssertEqual(cancelRequests, 0)
        XCTAssertEqual(model.selectedCount, 1)
        XCTAssertTrue(model.canSend)
        XCTAssertTrue(model.isReviewing)
        release.continuation.yield(())
        release.continuation.finish()
        for _ in 0..<40 {
            if !model.isReviewing { break }
            await Task.yield()
        }
        XCTAssertEqual(cancelRequests, 0)
        XCTAssertFalse(model.isReviewing)
        XCTAssertEqual(model.selectedCount, 1)
        XCTAssertTrue(model.canSend)
        XCTAssertFalse(model.requiresFreshReview)
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
        let gate = CaptureOperationGate(entered: entered)
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
        let uploadGate = CaptureOperationGate(entered: uploading)
        let receiptGate = CaptureOperationGate(entered: verifying)
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
        let gate = CaptureOperationGate(entered: entered)
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

    func testInvalidatedExportRequestsCleanupAfterItsStaleFileCompletes() async {
        let entered = expectation(description: "Export started")
        let cleanupCompleted = expectation(description: "Receipt-aware cleanup completed")
        let gate = CaptureOperationGate(entered: entered)
        let destination = URL(fileURLWithPath: "/tmp/stale-share-capture.jsonl")
        let export = CaptureExportModel(
            sourceExport: { _ in
                await gate.suspend()
                return destination
            },
            staleExportCleanup: {
                cleanupCompleted.fulfill()
            }
        )

        let pendingExport = Task { await export.export(source: URL(fileURLWithPath: "/tmp/local-capture.jsonl")) }
        await fulfillment(of: [entered], timeout: 2)
        export.invalidate()
        await gate.release()
        await pendingExport.value

        XCTAssertNil(export.fileURL, "The completed stale export must not become shareable")
        await fulfillment(of: [cleanupCompleted], timeout: 2)
    }
}

private actor CaptureOperationGate {
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
