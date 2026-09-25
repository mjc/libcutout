import XCTest
import Foundation
@testable import CutoutApp
import CutoutMobile
import CutoutMobileFFI

@MainActor
final class CaptureFeatureModelTests: XCTestCase {
    func testRejectedLabelKeepsAcceptedStateAndErrorSurvivesProgress() {
        var labelRequests = 0
        let capture = CaptureFeatureModel(changeCaptureLabel: { _, action in
            labelRequests += 1
            switch action {
            case .start(label: .lowBeamOn):
                return [.lowBeamOn]
            case .start:
                throw MobileCaptureAnnotationError.CapacityReached
            case .stop:
                return []
            }
        })
        capture.deliverCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/labels.jsonl")), origin: .manual)
        capture.startLabel(.lowBeamOn)
        capture.startLabel(.lowBeamOff)
        XCTAssertEqual(capture.activeLabels, [.lowBeamOn])
        XCTAssertNotNil(capture.annotationErrorText)
        XCTAssertTrue(capture.canAnnotate, "Stopping the accepted interval must remain available")
        for tick in 1...3 {
            capture.deliverCaptureEvent(.progress(CaptureProgress(
                elapsedMilliseconds: UInt64(tick * 1000), notificationCount: UInt64(tick),
                fileSizeBytes: 300, queuedMessageCount: 0, writerError: nil
            )))
            XCTAssertNotNil(capture.annotationErrorText)
        }
        XCTAssertEqual(capture.recordingSummary, "Receiving Bluetooth data")
        capture.stopLabel(.lowBeamOn)
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertNil(capture.annotationErrorText)
        XCTAssertEqual(labelRequests, 3)
    }

    func testWriterStartFailureWithoutStartedIsVisible() {
        let capture = CaptureFeatureModel()
        let generation = capture.sessionState.beginCapture(origin: .manual)!
        XCTAssertTrue(capture.sessionState.captureWriterStartFailed(generation: generation))
        capture.apply(.lifecycle(capture.sessionState.captureLifecycleSnapshot()))
        capture.apply(.failed(generation: CaptureGeneration(rawValue: generation.value)))
        XCTAssertEqual(capture.status, .failed)
        XCTAssertNil(capture.activeGeneration)
        XCTAssertTrue(capture.completed.isEmpty)
    }

    func testLabelsAreNotAcceptedBeforeTheWriterStartsOrAfterItCompletes() {
        var requests = 0
        let capture = CaptureFeatureModel(changeCaptureLabel: { _, _ in requests += 1; return [.ride] })
        capture.startLabel(.ride)
        XCTAssertEqual(requests, 0)
        let url = URL(fileURLWithPath: "/tmp/labels.jsonl")
        capture.deliverCaptureEvent(.started(fileURL: url))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        capture.startLabel(.ride)
        XCTAssertEqual(requests, 0)
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertEqual(capture.status, .saved(fileName: "labels.jsonl"))
    }

    func testFailedStartupPreservesThePreviousArtifactPresentation() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/saved.jsonl")
        capture.deliverCaptureEvent(.started(fileURL: url))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        XCTAssertFalse(capture.requestStart(device: nil, description: "new attempt") {
            let generation = capture.sessionState.beginCapture(origin: .manual)!
            XCTAssertTrue(capture.sessionState.captureWriterStartFailed(generation: generation))
            capture.apply(.lifecycle(capture.sessionState.captureLifecycleSnapshot()))
            capture.apply(.failed(generation: CaptureGeneration(rawValue: generation.value)))
            return false
        })
        XCTAssertEqual(capture.status, .saved(fileName: "saved.jsonl"))
        XCTAssertEqual(capture.completed.count, 1)
        XCTAssertEqual(capture.completed.first?.outcome, .saved)
        XCTAssertNil(capture.activeGeneration)
        XCTAssertTrue(capture.lifecycle.canStart)
    }

    func testQueuedStartPublicationsRetainEachDevicesIdentity() {
        let capture = CaptureFeatureModel()
        let firstDevice = CaptureDeviceIdentity(row: row(id: "a", advertisement: "GotWay_A"))
        let secondDevice = CaptureDeviceIdentity(row: row(id: "b", advertisement: "GotWay_B"))
        var publications: [CaptureEvent] = []
        func queueStart(device: CaptureDeviceIdentity, path: String) -> CaptureGeneration {
            var generation: MobileCaptureGenerationDto!
            XCTAssertTrue(capture.requestStart(device: device, description: nil) {
                generation = capture.sessionState.beginCapture(origin: .manual)!
                _ = capture.sessionState.captureWriterStarted(generation: generation)
                publications += [
                    .lifecycle(capture.sessionState.captureLifecycleSnapshot()),
                    .started(generation: CaptureGeneration(rawValue: generation.value), fileURL: URL(fileURLWithPath: path)),
                ]
                return true
            })
            return CaptureGeneration(rawValue: generation.value)
        }
        let first = queueStart(device: firstDevice, path: "/tmp/a")
        _ = capture.sessionState.retireCaptureWriter(generation: first.dto)
        let second = queueStart(device: secondDevice, path: "/tmp/b")
        publications.forEach(capture.apply)
        capture.deliverCaptureEvent(.finished(generation: first, fileURL: URL(fileURLWithPath: "/tmp/a")))
        XCTAssertEqual(capture.completed.first?.device, firstDevice)
        XCTAssertEqual(capture.device, secondDevice)
        XCTAssertEqual(capture.activeGeneration, second)
    }

    func testSameModelDevicesKeepAdvertisementAndPlatformIdentity() {
        let first = row(id: "device-a", advertisement: "GotWay_A")
        let second = row(id: "device-b", advertisement: "GotWay_B")
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/falcon.jsonl")
        XCTAssertTrue(capture.requestStart(device: CaptureDeviceIdentity(row: first), description: nil) {
            capture.deliverCaptureEvent(.started(fileURL: url), origin: .manual)
            return true
        })
        capture.updateDevice(second)
        capture.deliverCaptureEvent(.finished(fileURL: url))

        XCTAssertEqual(capture.completed.first?.device?.title, "Begode Falcon")
        XCTAssertEqual(capture.completed.first?.device?.advertisedName, "GotWay_A")
        XCTAssertEqual(capture.completed.first?.device?.platformIdentifier, "device-a")
        XCTAssertEqual(capture.completed.first?.isUserInitiated, true)
        XCTAssertNotEqual(CaptureDeviceIdentity(row: first), CaptureDeviceIdentity(row: second))
    }

    func testDetectedIdentityRefreshReachesCompletedArtifactWithoutAnotherPacket() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/automatic.jsonl")
        XCTAssertTrue(capture.requestStart(
            device: CaptureDeviceIdentity(row: row(id: "device-a", advertisement: "Old advertisement")), description: nil
        ) {
            capture.deliverCaptureEvent(.started(fileURL: url))
            return true
        })
        capture.updateDevice(row(id: "device-a", advertisement: "GotWay_A"))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        XCTAssertEqual(capture.completed.first?.device?.advertisedName, "GotWay_A")
        XCTAssertEqual(capture.completed.first?.isUserInitiated, false)
    }

    func testRecordingSummaryDoesNotExposeFileNamesOrLabelDiagnosticText() {
        let capture = CaptureFeatureModel()
        capture.deliverCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/private-name.jsonl")), origin: .manual)
        XCTAssertEqual(capture.recordingSummary, "Waiting for Bluetooth data")
        capture.deliverCaptureEvent(.notificationRecorded)
        capture.startLabel(.ride)
        XCTAssertEqual(capture.recordingSummary, "Receiving Bluetooth data")
        capture.beginSavingFixture()
        XCTAssertEqual(capture.recordingSummary, "Saving…")
    }

    func testQuickLabelActionNamesAndTonesFollowTheirState() {
        XCTAssertEqual(CaptureQuickLabel.ride.actionTitle(isActive: false), "Start Ride")
        XCTAssertEqual(CaptureQuickLabel.ride.actionTitle(isActive: true), "Stop Ride")
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.title, "Low beam on")
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.actionTitle(isActive: false), "Start Low beam on")
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.actionTitle(isActive: true), "Stop Low beam on")
        for label in CaptureQuickLabel.allCases {
            XCTAssertFalse(label.title.hasPrefix("capture.label."))
            XCTAssertFalse(label.actionTitle(isActive: false).hasPrefix("capture.label."))
            XCTAssertFalse(label.actionTitle(isActive: true).hasPrefix("capture.label."))
        }
        XCTAssertEqual(CaptureActionButtonTone.forState(isActive: false), .start)
        XCTAssertEqual(CaptureActionButtonTone.forState(isActive: true), .stop)
        XCTAssertEqual(CaptureActionButtonTone.finish, .finish)
    }

    func testCaptureStatusKeepsVisibleTextToneAndMeaningfulAnnouncements() {
        XCTAssertEqual(
            CaptureStatus.recordingLocally(fileName: "ride.cutout").displayText,
            "Recording locally: ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.recording(label: nil, notificationCount: 2, fileName: nil).displayText,
            "Recording: 2 notifications"
        )
        XCTAssertEqual(
            CaptureStatus.recording(label: "Ride", notificationCount: 3, fileName: "ride.cutout").displayText,
            "Ride: 3 notifications → ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.labelStarted(label: "Ride", notificationCount: 3, fileName: "ride.cutout").displayText,
            "Ride started: 3 notifications → ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.labelStopped(label: "Ride", notificationCount: 4, fileName: nil).displayText,
            "Ride stopped"
        )
        XCTAssertEqual(CaptureStatus.saved(fileName: "ride.cutout").displayText, "Saved capture: ride.cutout")
        XCTAssertEqual(CaptureStatus.failed.statusStripTone, .critical)
        XCTAssertEqual(CaptureStatus.recordingLocally(fileName: "capture.jsonl").statusStripTone, .nominal)
        XCTAssertEqual(
            CaptureStatus.labelStarted(label: "Ride", notificationCount: 3, fileName: "ride.cutout")
                .accessibilityAnnouncement,
            "Ride capture started"
        )
        XCTAssertEqual(
            CaptureStatus.labelStopped(label: "Ride", notificationCount: 4, fileName: "ride.cutout")
                .accessibilityAnnouncement,
            "Ride capture stopped"
        )
        XCTAssertEqual(CaptureStatus.saved(fileName: "ride.cutout").accessibilityAnnouncement, "Capture saved")
        XCTAssertEqual(CaptureStatus.failed.accessibilityAnnouncement, "Capture failed")
        XCTAssertNil(CaptureStatus.recording(label: "Ride", notificationCount: 200, fileName: "ride.cutout").accessibilityAnnouncement)
        XCTAssertNil(CaptureStatus.recordingLocally(fileName: "ride.cutout").accessibilityAnnouncement)
    }

    func testTypedCaptureEventsAndProgressUpdateFeaturePresentation() {
        let capture = CaptureFeatureModel()
        let fileURL = URL(fileURLWithPath: "/tmp/ride.cutout")
        let progress = CaptureProgress(
            elapsedMilliseconds: 63_000,
            notificationCount: 42,
            fileSizeBytes: 12_288,
            queuedMessageCount: 2,
            writerError: nil
        )

        capture.deliverCaptureEvent(.started(fileURL: fileURL), origin: .manual)
        XCTAssertEqual(capture.status, .recordingLocally(fileName: "ride.cutout"))
        capture.deliverCaptureEvent(.progress(progress))

        XCTAssertEqual(capture.progress, progress)
        XCTAssertEqual(capture.status, .recording(label: nil, notificationCount: 42, fileName: "ride.cutout"))
        capture.deliverCaptureEvent(.notificationRecorded)
        XCTAssertEqual(capture.status, .recording(label: nil, notificationCount: 43, fileName: "ride.cutout"))
        capture.deliverCaptureEvent(.finished(fileURL: fileURL))
        XCTAssertEqual(capture.status, .saved(fileName: "ride.cutout"))

        capture.deliverCaptureEvent(.started(fileURL: fileURL), origin: .manual)
        capture.deliverCaptureEvent(.failed)
        XCTAssertEqual(capture.status, .failed)
    }

    func testLateTerminalEventsCannotReplaceTheCurrentGeneration() {
        let capture = CaptureFeatureModel()
        let first = CaptureGeneration(rawValue: 1)
        let second = CaptureGeneration(rawValue: 2)
        let firstURL = URL(fileURLWithPath: "/tmp/first.cutout")
        let secondURL = URL(fileURLWithPath: "/tmp/second.cutout")
        let progress = CaptureProgress(
            elapsedMilliseconds: 1_000,
            notificationCount: 7,
            fileSizeBytes: 128,
            queuedMessageCount: 0,
            writerError: nil
        )

        capture.deliverCaptureEvent(.started(generation: first, fileURL: firstURL), origin: .manual)
        capture.deliverCaptureEvent(.started(generation: second, fileURL: secondURL), origin: .manual)
        capture.deliverCaptureEvent(.progress(generation: second, progress))
        capture.deliverCaptureEvent(.finished(generation: first, fileURL: firstURL))
        capture.deliverCaptureEvent(.failed(generation: first))

        XCTAssertEqual(
            capture.status,
            .recording(label: nil, notificationCount: 7, fileName: "second.cutout")
        )
    }

    func testInvalidRepeatedLabelTransitionsAreIgnored() {
        var active = [MobileCaptureLabelDto]()
        var requestedActions = [MobileCaptureLabelActionDto]()
        let capture = CaptureFeatureModel(changeCaptureLabel: { _, action in
            requestedActions.append(action)
            switch action {
            case let .start(label): active = [label]
            case .stop: active = []
            }
            return active
        })

        capture.stopLabel(.ride)
        XCTAssertNil(capture.status?.displayText)
        XCTAssertTrue(capture.activeLabels.isEmpty)
        capture.deliverCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/labels.jsonl")), origin: .manual)
        capture.startLabel(.ride)
        XCTAssertEqual(capture.status, .labelStarted(label: "Ride", notificationCount: 0, fileName: "labels.jsonl"))
        XCTAssertEqual(capture.activeLabels, [.ride])
        capture.startLabel(.ride)
        capture.stopLabel(.ride)
        capture.stopLabel(.ride)

        XCTAssertEqual(capture.status, .labelStopped(label: "Ride", notificationCount: 0, fileName: "labels.jsonl"))
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertEqual(requestedActions.count, 2)
    }

    func testFlushFailureAffectsOnlyAnActiveCapture() async {
        let sessionState = CutoutSessionStateHandle()
        var flushCalls = 0
        let capture = CaptureFeatureModel(sessionState: sessionState, flush: {
            flushCalls += 1
            if let attempt = sessionState.captureLifecycleSnapshot().attempt {
                _ = sessionState.captureWriterFailed(generation: attempt.generation)
            }
            return false
        })

        let inactiveFlushSucceeded = await capture.flush()
        XCTAssertFalse(inactiveFlushSucceeded)
        XCTAssertNil(capture.status)

        capture.deliverCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/flush.jsonl")), origin: .manual)
        let activeFlushSucceeded = await capture.flush()
        XCTAssertFalse(activeFlushSucceeded)
        XCTAssertEqual(capture.status, .failed)
        XCTAssertEqual(flushCalls, 2)
    }

    func testCaptureSessionDetailsExposeTypedWriterHealth() {
        let healthyProgress = CaptureProgress(
            elapsedMilliseconds: 63_000,
            notificationCount: 42,
            fileSizeBytes: 12_288,
            queuedMessageCount: 0,
            writerError: nil
        )
        let failedProgress = CaptureProgress(
            elapsedMilliseconds: 63_000,
            notificationCount: 42,
            fileSizeBytes: 12_288,
            queuedMessageCount: 0,
            writerError: "queue overrun"
        )
        let healthyRows = captureSessionDetailRows(progress: healthyProgress)
        let failedRows = captureSessionDetailRows(progress: failedProgress)

        XCTAssertEqual(healthyProgress.writerHealth, .healthy)
        XCTAssertEqual(failedProgress.writerHealth, .failed)
        XCTAssertEqual(healthyRows[0].metricValue, healthyProgress.elapsedMetricValue)
        XCTAssertEqual(healthyRows[1].metricValue, healthyProgress.notificationCountMetricValue)
        XCTAssertEqual(healthyRows[2].metricValue, healthyProgress.fileSizeMetricValue)
        XCTAssertEqual(healthyRows[3].metricValue, healthyProgress.queuedMessageCountMetricValue)
        XCTAssertEqual(
            healthyRows[4].metricValue,
            healthyProgress.writerHealth.metricValue(display: "Healthy")
        )
        XCTAssertEqual(
            failedRows[4].metricValue,
            failedProgress.writerHealth.metricValue(display: "Failed")
        )
        XCTAssertEqual(
            healthyRows.map(\.id),
            [
                "capture-elapsed",
                "capture-packets",
                "capture-file-size",
                "capture-queued-messages",
                "capture-writer-health",
            ]
        )
        XCTAssertEqual(healthyRows[3].label, "Pending writes")
        XCTAssertEqual(healthyRows[3].accessibilityValueText, "0")
        XCTAssertEqual(healthyRows.last?.label, "Writer")
        XCTAssertEqual(healthyRows.last?.accessibilityValueText, "Healthy")
        XCTAssertEqual(failedRows.last?.accessibilityValueText, "Failed")
    }

    func testDismissedCaptureRouteDoesNotCancelInFlightFinish() async {
        let sessionState = CutoutSessionStateHandle()
        let generation = sessionState.beginCapture(origin: .manual)!
        XCTAssertTrue(sessionState.captureWriterStarted(generation: generation))
        let finishEntered = expectation(description: "finish remains in flight")
        var releaseFlush: CheckedContinuation<Bool, Never>?
        var finishCalls = 0
        let capture = CaptureFeatureModel(sessionState: sessionState, finish: {
            finishCalls += 1
            guard let token = sessionState.beginCaptureFinish(generation: generation) else { return false }
            finishEntered.fulfill()
            let flushed = await withCheckedContinuation { releaseFlush = $0 }
            return sessionState.finishCaptureFlush(token: token, succeeded: flushed)
        })
        capture.deliverCaptureEvent(.started(
            generation: .init(rawValue: generation.value),
            fileURL: URL(fileURLWithPath: "/tmp/dismissed-route.jsonl")
        ), origin: .manual)

        var route: CaptureRouteView? = CaptureRouteView(capture: capture)
        XCTAssertNotNil(route)
        let viewWaiter = Task { @MainActor in await capture.finish() }
        await fulfillment(of: [finishEntered], timeout: 2)
        route = nil
        viewWaiter.cancel()

        XCTAssertTrue(capture.isFinishing)
        XCTAssertEqual(finishCalls, 1)
        XCTAssertEqual(capture.activeGeneration, .init(rawValue: generation.value))
        releaseFlush?.resume(returning: true)
        let finishSucceeded = await viewWaiter.value
        XCTAssertTrue(finishSucceeded)
        XCTAssertEqual(finishCalls, 1)

        capture.deliverCaptureEvent(.finished(
            generation: .init(rawValue: generation.value),
            fileURL: URL(fileURLWithPath: "/tmp/dismissed-route.jsonl")
        ))
        XCTAssertFalse(capture.isFinishing)
    }

    func testConcurrentFinishRequestsShareOneFlushAndDisconnectAfterSuccess() async {
        let sessionState = CutoutSessionStateHandle()
        let generation = sessionState.beginCapture(origin: .manual)!
        XCTAssertTrue(sessionState.captureWriterStarted(generation: generation))
        let enteredFinish = expectation(description: "finish begins")
        var releaseFlush: CheckedContinuation<Bool, Never>?
        var finishCalls = 0
        var disconnectCalls = 0
        let capture = CaptureFeatureModel(sessionState: sessionState, finish: {
            finishCalls += 1
            guard let token = sessionState.beginCaptureFinish(generation: generation) else { return false }
            enteredFinish.fulfill()
            let flushed = await withCheckedContinuation { releaseFlush = $0 }
            let accepted = sessionState.finishCaptureFlush(token: token, succeeded: flushed)
            if accepted { disconnectCalls += 1 }
            return accepted
        })
        capture.deliverCaptureEvent(.started(generation: .init(rawValue: generation.value), fileURL: URL(fileURLWithPath: "/tmp/finish.jsonl")), origin: .manual)

        let first = Task { @MainActor in await capture.finish() }
        await fulfillment(of: [enteredFinish], timeout: 2)
        let second = Task { @MainActor in await capture.finish() }
        await Task.yield()

        XCTAssertTrue(capture.isFinishing)
        XCTAssertEqual(finishCalls, 1)
        XCTAssertEqual(disconnectCalls, 0)
        releaseFlush?.resume(returning: true)

        let firstSucceeded = await first.value
        let secondSucceeded = await second.value
        XCTAssertTrue(firstSucceeded)
        XCTAssertTrue(secondSucceeded)
        XCTAssertEqual(finishCalls, 1)
        XCTAssertEqual(disconnectCalls, 1)
        capture.deliverCaptureEvent(.finished(
            generation: .init(rawValue: generation.value),
            fileURL: URL(fileURLWithPath: "/tmp/finish.jsonl")
        ))
        XCTAssertFalse(capture.isFinishing)
    }

    func testFailedFinishCanRetryWithoutLosingTheCapture() async {
        let sessionState = CutoutSessionStateHandle()
        let generation = sessionState.beginCapture(origin: .manual)!
        XCTAssertTrue(sessionState.captureWriterStarted(generation: generation))
        var flushOutcomes = [false, true]
        var disconnectCalls = 0
        let capture = CaptureFeatureModel(sessionState: sessionState, finish: {
            guard let token = sessionState.beginCaptureFinish(generation: generation),
                  !flushOutcomes.isEmpty else { return false }
            let flushed = flushOutcomes.removeFirst()
            let accepted = sessionState.finishCaptureFlush(token: token, succeeded: flushed)
            if accepted { disconnectCalls += 1 }
            return accepted
        })
        capture.deliverCaptureEvent(.started(generation: .init(rawValue: generation.value), fileURL: URL(fileURLWithPath: "/tmp/retry.jsonl")), origin: .manual)

        let firstSucceeded = await capture.finish()
        XCTAssertFalse(firstSucceeded)
        XCTAssertFalse(capture.isFinishing)
        XCTAssertEqual(capture.activeGeneration, .init(rawValue: generation.value))
        XCTAssertEqual(capture.status, .failed)
        XCTAssertEqual(disconnectCalls, 0)

        let retrySucceeded = await capture.finish()
        XCTAssertTrue(retrySucceeded)
        XCTAssertEqual(disconnectCalls, 1)
    }

    func testNewGenerationDoesNotJoinAnOlderFinishTask() async throws {
        let sessionState = CutoutSessionStateHandle()
        let firstGeneration = sessionState.beginCapture(origin: .manual)!
        XCTAssertTrue(sessionState.captureWriterStarted(generation: firstGeneration))
        let firstFinishEntered = expectation(description: "first finish waits on its flush")
        var releaseFirstFlush: CheckedContinuation<Bool, Never>?
        var finishCalls = 0
        let capture = CaptureFeatureModel(sessionState: sessionState, finish: {
            finishCalls += 1
            guard let attempt = sessionState.captureLifecycleSnapshot().attempt,
                  let token = sessionState.beginCaptureFinish(generation: attempt.generation) else { return false }
            if attempt.generation.value == firstGeneration.value {
                firstFinishEntered.fulfill()
                let flushed = await withCheckedContinuation { releaseFirstFlush = $0 }
                return sessionState.finishCaptureFlush(token: token, succeeded: flushed)
            }
            return sessionState.finishCaptureFlush(token: token, succeeded: true)
        })
        capture.deliverCaptureEvent(.started(
            generation: .init(rawValue: firstGeneration.value),
            fileURL: URL(fileURLWithPath: "/tmp/first-finish.jsonl")
        ), origin: .manual)

        let firstFinish = Task { @MainActor in await capture.finish() }
        await fulfillment(of: [firstFinishEntered], timeout: 2)
        capture.deliverCaptureEvent(.started(
            generation: .init(rawValue: 0),
            fileURL: URL(fileURLWithPath: "/tmp/second-finish.jsonl")
        ), origin: .manual)

        let secondFinishSucceeded = await capture.finish()
        XCTAssertTrue(secondFinishSucceeded)
        XCTAssertEqual(finishCalls, 2)
        let secondGeneration = try! XCTUnwrap(capture.activeGeneration)
        capture.deliverCaptureEvent(.finished(
            generation: secondGeneration,
            fileURL: URL(fileURLWithPath: "/tmp/second-finish.jsonl")
        ))
        XCTAssertFalse(capture.isFinishing)

        releaseFirstFlush?.resume(returning: true)
        let firstFinishSucceeded = await firstFinish.value
        XCTAssertFalse(firstFinishSucceeded)
        XCTAssertEqual(capture.completed.first?.id, secondGeneration)
        XCTAssertEqual(capture.status, .saved(fileName: "second-finish.jsonl"))
        XCTAssertFalse(capture.isFinishing)
    }

    private func row(id: String, advertisement: String) -> DevicePickerRow {
        DevicePickerRow(
            id: id, title: "Begode Falcon", subtitle: "", detail: "",
            state: .supported(action: "Connect"), symbolName: "circle",
            advertisedName: advertisement
        )
    }

    func testFinishedCaptureRetainsArtifactAndOldCompletionDoesNotReplaceNewRecording() {
        let capture = CaptureFeatureModel()
        let first = CaptureGeneration(rawValue: 1)
        let second = CaptureGeneration(rawValue: 2)
        let firstURL = URL(fileURLWithPath: "/tmp/first.jsonl")
        let secondURL = URL(fileURLWithPath: "/tmp/second.jsonl")
        capture.deliverCaptureEvent(.started(generation: first, fileURL: firstURL))
        capture.deliverCaptureEvent(.started(generation: second, fileURL: secondURL))
        capture.deliverCaptureEvent(.finished(generation: first, fileURL: firstURL))

        XCTAssertEqual(capture.activeGeneration, second)
        XCTAssertEqual(capture.status, .recordingLocally(fileName: "second.jsonl"))
        XCTAssertEqual(capture.completed.first?.fileURL, firstURL)
        XCTAssertEqual(capture.completed.first?.outcome, .saved)
    }

    func testFailureRetainsPartialArtifactWithoutCallingItSaved() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/partial.jsonl")
        capture.deliverCaptureEvent(.started(fileURL: url))
        capture.deliverCaptureEvent(.failed)
        XCTAssertEqual(capture.completed.first?.fileURL, url)
        XCTAssertEqual(capture.completed.first?.outcome, .failed)
        XCTAssertEqual(capture.status, .failed)
        XCTAssertNil(capture.activeGeneration)
    }

    func testNewGenerationResetsProgressLabelsAndFinishAdmission() {
        let capture = CaptureFeatureModel()
        capture.deliverCaptureEvent(.started(generation: .init(rawValue: 1), fileURL: URL(fileURLWithPath: "/tmp/a")), origin: .manual)
        capture.startLabel(.ride)
        capture.beginSavingFixture()
        capture.deliverCaptureEvent(.started(generation: .init(rawValue: 2), fileURL: URL(fileURLWithPath: "/tmp/b")))
        XCTAssertNil(capture.progress)
        XCTAssertTrue(capture.activeLabels.isEmpty)
        XCTAssertFalse(capture.isFinishing)
    }

    func testDuplicateTerminalCallbackDoesNotDuplicateSavedArtifact() {
        let capture = CaptureFeatureModel()
        let url = URL(fileURLWithPath: "/tmp/a")
        capture.deliverCaptureEvent(.started(fileURL: url))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        capture.deliverCaptureEvent(.finished(fileURL: url))
        XCTAssertEqual(capture.completed.count, 1)
    }
}
