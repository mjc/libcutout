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
}
