import CutoutMobile
import XCTest
@testable import CutoutApp

@MainActor
final class CameraMediaModelTests: XCTestCase {
    func testMediaActionsRejectInvalidPortBeforeStartingWork() {
        let model = CameraMediaModel()
        let media = CameraMediaEvidence(
            name: "clip.TS",
            path: "A:\\Novatek\\Movie\\clip.TS",
            sizeBytes: 42,
            timecode: 7,
            time: "2025/01/01 00:00:00",
            attributes: 32
        )

        model.downloadMedia(media, address: "192.168.1.254", port: "invalid")

        XCTAssertEqual(model.mediaErrorKey, "camera.error.invalid_port")
        XCTAssertNil(model.downloadingMediaPath)

        model.fetchThumbnail(media, address: "192.168.1.254", port: "65536")

        XCTAssertEqual(model.thumbnailErrorKey, "camera.error.invalid_port")
        XCTAssertNil(model.thumbnailPathInFlight)
    }
}
