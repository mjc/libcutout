import XCTest
@testable import CutoutMobile
import CutoutMobileFFI

final class CameraH264AccessUnitTests: XCTestCase {
    func testAccessUnitExtractsParameterSetsAndRandomAccessPoint() throws {
        let accessUnit = try CameraH264AccessUnit(data: Data([
            0, 0, 0, 2, 0x67, 0x64,
            0, 0, 0, 2, 0x68, 0xee,
            0, 0, 0, 2, 0x65, 0x88,
        ]))

        XCTAssertEqual(accessUnit.parameterSets, [Data([0x67, 0x64]), Data([0x68, 0xee])])
        XCTAssertTrue(accessUnit.isRandomAccessPoint)
        XCTAssertEqual(accessUnit.data.count, 18)
    }

    func testAccessUnitRejectsTruncatedNal() {
        XCTAssertThrowsError(try CameraH264AccessUnit(data: Data([0, 0, 0, 4, 0x67]))) { error in
            XCTAssertEqual(error as? CameraH264AccessUnitError, .truncatedNAL)
        }
    }

    func testAccessUnitRejectsZeroLengthNal() {
        XCTAssertThrowsError(try CameraH264AccessUnit(data: Data([0, 0, 0, 0]))) { error in
            XCTAssertEqual(error as? CameraH264AccessUnitError, .zeroLengthNAL)
        }
    }

    @MainActor
    func testRendererWaitsForParameterSetsBeforeEnqueueing() async {
        let renderer = CameraPreviewRenderer()
        let frame = MobileCameraVideoFrameDto(
            data: Data([0, 0, 0, 2, 0x65, 0x88]),
            loss: 0,
            isRandomAccessPoint: true,
            timestamp: 1,
            clockRateHz: 90_000
        )

        do {
            try await renderer.enqueue(frame)
            XCTFail("frames without SPS/PPS must not be enqueued")
        } catch {
            XCTAssertEqual(error as? CameraPreviewRendererError, .missingParameterSets)
        }
    }

    @MainActor
    func testRendererAcceptsR3H264ParameterSets() async throws {
        let renderer = CameraPreviewRenderer()
        let frame = MobileCameraVideoFrameDto(
            data: Data([
                0, 0, 0, 23,
                0x67, 0x64, 0x00, 0x33, 0xac, 0x15, 0x4a, 0x0d,
                0x43, 0xda, 0x6e, 0x02, 0x02, 0x02, 0x80, 0x00,
                0x01, 0xf4, 0x00, 0x00, 0x75, 0x30, 0x02,
                0, 0, 0, 4, 0x68, 0xee, 0x3c, 0xb0,
                0, 0, 0, 3, 0x65, 0x88, 0x80,
            ]),
            loss: 0,
            isRandomAccessPoint: true,
            timestamp: 0,
            clockRateHz: 90_000
        )

        try await renderer.enqueue(frame)
    }
}
