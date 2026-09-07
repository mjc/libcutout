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
}
