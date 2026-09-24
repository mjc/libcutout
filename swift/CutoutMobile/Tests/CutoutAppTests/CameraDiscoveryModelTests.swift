import CutoutMobile
import XCTest
@testable import CutoutApp

@MainActor
final class CameraDiscoveryModelTests: XCTestCase {
    func testInvalidPortDoesNotStartDiscoveryOrClearMediaEvidence() {
        var loadCount = 0
        var refreshCount = 0
        let model = CameraDiscoveryModel(
            loadEvidence: { _, _ in
                loadCount += 1
                return Self.evidence
            },
            prepareForEvidenceRefresh: { refreshCount += 1 }
        )

        XCTAssertNil(model.load(address: "192.168.1.254", port: "65536"))
        XCTAssertEqual(model.readErrorKey, "camera.error.invalid_port")
        XCTAssertFalse(model.isReading)
        XCTAssertEqual(loadCount, 0)
        XCTAssertEqual(refreshCount, 0)
    }

    func testSuccessfulDiscoveryPreparesMediaAndAnnotatesEvidence() async {
        var requestedOrigin: (String, UInt16)?
        var annotations: [(String, String)] = []
        var refreshCount = 0
        let model = CameraDiscoveryModel(
            loadEvidence: { address, port in
                requestedOrigin = (address, port)
                return Self.evidence
            },
            prepareForEvidenceRefresh: { refreshCount += 1 },
            annotateCapture: { annotations.append(($0, $1)) }
        )

        let task = model.load(address: "192.168.1.254", port: "8080")
        XCTAssertTrue(model.isReading)
        XCTAssertNil(model.readErrorKey)
        await task?.value

        XCTAssertEqual(requestedOrigin?.0, "192.168.1.254")
        XCTAssertEqual(requestedOrigin?.1, 8080)
        XCTAssertEqual(refreshCount, 1)
        XCTAssertEqual(annotations.map(\.0), ["camera_profile", "camera_firmware"])
        XCTAssertEqual(annotations.map(\.1), ["novatek_r3_pro", Self.evidence.firmwareVersion])
        XCTAssertFalse(model.isReading)
        XCTAssertNil(model.readErrorKey)
    }

    func testUnavailableWiFiMapsToConnectionError() async {
        var permissionRequired = false
        let model = CameraDiscoveryModel(
            loadEvidence: { _, _ in throw CameraReadOnlyRequestError.pathUnavailable },
            observePermissionRequired: { permissionRequired = true }
        )

        await model.load(address: "192.168.1.254", port: "80")?.value

        XCTAssertEqual(model.readErrorKey, "camera.connection.detail.wifi_required")
        XCTAssertFalse(permissionRequired)
        XCTAssertFalse(model.isReading)
    }

    func testLocalNetworkPermissionFailureUpdatesAdapterAndReadError() async {
        var permissionRequired = false
        let model = CameraDiscoveryModel(
            loadEvidence: { _, _ in throw URLError(.notConnectedToInternet) },
            observePermissionRequired: { permissionRequired = true }
        )

        await model.load(address: "192.168.1.254", port: "80")?.value

        XCTAssertTrue(permissionRequired)
        XCTAssertEqual(model.readErrorKey, "camera.error.read_failed")
        XCTAssertFalse(model.isReading)
    }

    func testSupersededLoadCannotAnnotateOrClearCurrentReadState() async {
        var firstContinuation: CheckedContinuation<CameraReadOnlyEvidence, Error>?
        var secondContinuation: CheckedContinuation<CameraReadOnlyEvidence, Error>?
        var loadCount = 0
        var annotations: [(String, String)] = []
        let model = CameraDiscoveryModel(
            loadEvidence: { _, _ in
                loadCount += 1
                return try await withCheckedThrowingContinuation { continuation in
                    if loadCount == 1 {
                        firstContinuation = continuation
                    } else {
                        secondContinuation = continuation
                    }
                }
            },
            annotateCapture: { annotations.append(($0, $1)) }
        )

        let firstTask = model.load(address: "192.168.1.254", port: "80")
        await waitUntil { firstContinuation != nil }
        let secondTask = model.load(address: "192.168.1.254", port: "80")
        await waitUntil { secondContinuation != nil }

        firstContinuation?.resume(throwing: CameraReadOnlyRequestError.pathUnavailable)
        await firstTask?.value
        XCTAssertTrue(model.isReading)
        XCTAssertNil(model.readErrorKey)
        XCTAssertTrue(annotations.isEmpty)

        secondContinuation?.resume(returning: Self.evidence)
        await secondTask?.value
        XCTAssertFalse(model.isReading)
        XCTAssertNil(model.readErrorKey)
        XCTAssertEqual(annotations.count, 2)
    }

    func testUnsupportedProfileMapsToProfileError() async {
        let model = CameraDiscoveryModel(
            loadEvidence: { _, _ in throw CameraReadOnlyRequestError.unsupportedProfile }
        )

        await model.load(address: "192.168.1.254", port: "80")?.value

        XCTAssertEqual(model.readErrorKey, "camera.error.unsupported_profile")
        XCTAssertFalse(model.isReading)
    }

    private func waitUntil(
        file: StaticString = #filePath,
        line: UInt = #line,
        condition: () -> Bool
    ) async {
        for _ in 0 ..< 100 where !condition() {
            await Task.yield()
        }
        XCTAssertTrue(condition(), file: file, line: line)
    }

    private static let evidence = CameraReadOnlyEvidence(
        firmwareVersion: "R3V1.1_20240411",
        movieRTSPURI: "rtsp://192.168.1.254/live",
        photoRTSPURI: "rtsp://192.168.1.254/photo",
        configuration: [],
        storagePresent: true,
        media: []
    )
}
