import XCTest
@testable import CutoutMobile

final class CameraLocalNetworkAdapterTests: XCTestCase {
    func testCameraPathRequiresWiFiBeforeCameraConfiguration() {
        XCTAssertEqual(
            cameraConnectionPresentation(pathStatus: .unavailable, usesWiFi: false),
            .wifiRequired
        )
        XCTAssertEqual(
            cameraConnectionPresentation(pathStatus: .satisfied, usesWiFi: false),
            .wifiRequired
        )
    }

    func testSatisfiedWiFiPathDoesNotPretendTheCameraIsConnected() {
        XCTAssertEqual(
            cameraConnectionPresentation(pathStatus: .satisfied, usesWiFi: true),
            .notConfigured
        )
    }

    @MainActor
    func testAdapterStartAndStopRemainNonOptimistic() {
        let adapter = CameraLocalNetworkAdapter()

        adapter.start()
        adapter.stop()

        XCTAssertEqual(adapter.presentation, .initial)
    }
}
