import XCTest
@testable import CutoutMobile

final class CameraPresentationTests: XCTestCase {
    func testInitialCameraPresentationDoesNotClaimConnectionOrRecording() {
        let presentation = CameraPresentation.initial

        XCTAssertEqual(presentation.connection, .notConfigured)
        XCTAssertEqual(presentation.preview, .stopped)
        XCTAssertEqual(presentation.recording, .unknown)
        XCTAssertEqual(presentation.storage, .unknown)
        XCTAssertNil(presentation.profileName)
    }

    func testPreviewAndOnboardRecordingAreIndependentPresentationValues() {
        let presentation = CameraPresentation(
            connection: .connected,
            preview: .stopped,
            recording: .recording,
            storage: .present,
            profileName: "FreedConn R3 Pro · Novatek"
        )

        XCTAssertEqual(presentation.preview, .stopped)
        XCTAssertEqual(presentation.recording, .recording)
        XCTAssertEqual(presentation.storage, .present)
    }

    func testReadOnlyEvidenceExposesBoundedMediaCountForPresentation() {
        let evidence = CameraReadOnlyEvidence(
            firmwareVersion: "FW-1.0",
            movieRTSPURI: "rtsp://192.168.1.254/live",
            photoRTSPURI: "rtsp://192.168.1.254/photo",
            configuration: [],
            storagePresent: true,
            media: [
                CameraMediaEvidence(
                    name: "MOV001.TS",
                    path: "/DCIM/MOV001.TS",
                    sizeBytes: 42,
                    timecode: 1,
                    time: "2026-09-07 12:00:00",
                    attributes: 0
                )
            ]
        )

        XCTAssertEqual(evidence.mediaCount, 1)
    }

    func testReadOnlyEvidenceOnlyAdvertisesOnboardRecordingForCommand2001StatusZero() {
        let supported = CameraReadOnlyEvidence(
            firmwareVersion: "FW-1.0",
            movieRTSPURI: "rtsp://192.168.1.254/live",
            photoRTSPURI: "rtsp://192.168.1.254/photo",
            configuration: [
                CameraCommandStatusEvidence(commandID: 2001, status: 0),
                CameraCommandStatusEvidence(commandID: 2002, status: 11)
            ],
            storagePresent: true,
            media: []
        )
        let unavailable = CameraReadOnlyEvidence(
            firmwareVersion: "FW-1.0",
            movieRTSPURI: "rtsp://192.168.1.254/live",
            photoRTSPURI: "rtsp://192.168.1.254/photo",
            configuration: [CameraCommandStatusEvidence(commandID: 2001, status: 1)],
            storagePresent: true,
            media: []
        )

        XCTAssertTrue(supported.supportsOnboardRecording)
        XCTAssertFalse(unavailable.supportsOnboardRecording)
    }

    func testReadOnlyEvidenceOnlyAdvertisesStillCaptureForCommand1001StatusZero() {
        let supported = CameraReadOnlyEvidence(
            firmwareVersion: "FW-1.0",
            movieRTSPURI: "rtsp://192.168.1.254/live",
            photoRTSPURI: "rtsp://192.168.1.254/photo",
            configuration: [CameraCommandStatusEvidence(commandID: 1001, status: 0)],
            storagePresent: true,
            media: []
        )
        let unavailable = CameraReadOnlyEvidence(
            firmwareVersion: "FW-1.0",
            movieRTSPURI: "rtsp://192.168.1.254/live",
            photoRTSPURI: "rtsp://192.168.1.254/photo",
            configuration: [CameraCommandStatusEvidence(commandID: 1001, status: 1)],
            storagePresent: true,
            media: []
        )

        XCTAssertTrue(supported.supportsStillCapture)
        XCTAssertFalse(unavailable.supportsStillCapture)
    }

    func testReadOnlyEvidenceOnlyAdvertisesMediaThumbnailsForCommand4001StatusZero() {
        let evidence = CameraReadOnlyEvidence(
            firmwareVersion: "FW-1.0",
            movieRTSPURI: "rtsp://192.168.1.254/live",
            photoRTSPURI: "rtsp://192.168.1.254/photo",
            configuration: [CameraCommandStatusEvidence(commandID: 4001, status: 0)],
            storagePresent: true,
            media: []
        )

        XCTAssertTrue(evidence.supportsMediaThumbnails)
    }

    func testCameraMediaReferenceKeepsRideAssociationAndClockUncertaintyExplicit() {
        let reference = CameraMediaReference(
            cameraPath: #"A:\Novatek\Movie\clip.TS"#,
            localURL: URL(fileURLWithPath: "/tmp/clip.TS"),
            sizeBytes: 42,
            cameraTimecode: 7,
            cameraTime: "2025/01/01 00:00:00",
            rideCaptureFileName: "ride.jsonl",
            clockUncertainty: .unknown
        )

        XCTAssertEqual(reference.cameraPath, #"A:\Novatek\Movie\clip.TS"#)
        XCTAssertEqual(reference.rideCaptureFileName, "ride.jsonl")
        XCTAssertEqual(reference.clockUncertainty, .unknown)
    }
}
