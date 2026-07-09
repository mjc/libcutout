import XCTest
@testable import CutoutApp
import CutoutMobile

final class CutoutAppRouteTests: XCTestCase {
    deinit {}

    func testPreviewArgumentsResolveToPreviewRoute() {
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp", "--preview-screen", PevScreenID.vescDebug.rawValue],
                environment: [:]
            ),
            .vescDebug
        )
    }

    func testPreviewEnvironmentResolvesToPreviewRoute() {
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp"],
                environment: ["CUTOUT_PREVIEW_SCREEN": PevScreenID.liveActivity.rawValue]
            ),
            .liveActivity
        )
    }

    func testPreviewArgumentsWinOverEnvironment() {
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp", "--preview-screen", PevScreenID.vescDebug.rawValue],
                environment: ["CUTOUT_PREVIEW_SCREEN": PevScreenID.liveActivity.rawValue]
            ),
            .vescDebug
        )
    }

    func testInvalidPreviewInputFallsBackToDevicePicker() {
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp", "--preview-screen", "not-real"],
                environment: ["CUTOUT_PREVIEW_SCREEN": "also-not-real"]
            ),
            .devicePicker
        )
    }

    func testScreenRoutesMatchTopLevelSections() {
        XCTAssertEqual(CutoutAppRoute.route(for: .devicePicker), .devicePicker)
        XCTAssertEqual(CutoutAppRoute.route(for: .eucRide), .eucRide)
        XCTAssertEqual(CutoutAppRoute.route(for: .eucMap), .eucMap)
        XCTAssertEqual(CutoutAppRoute.route(for: .eucTune), .eucTune)
        XCTAssertEqual(CutoutAppRoute.route(for: .liveActivity), .liveActivity)
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsOverview), .eucPack(.bmsOverview))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellMap6S), .eucPack(.bmsCellMap6S))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellMap40S), .eucPack(.bmsCellMap40S))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellDetail), .eucPack(.bmsCellDetail))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsUnknownTopology), .eucPack(.bmsUnknownTopology))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsNoData), .eucPack(.bmsNoData))
        XCTAssertEqual(CutoutAppRoute.route(for: .eucGarage), .eucPack(.eucGarage))
        XCTAssertEqual(CutoutAppRoute.route(for: .vescDebug), .vescDebug)
        XCTAssertEqual(CutoutAppRoute.route(for: .vescMap), .vescMap)
        XCTAssertEqual(CutoutAppRoute.route(for: .vescLogs), .vescLogs)
    }

    func testConnectionRoutesMatchRideSections() {
        XCTAssertEqual(CutoutAppRoute.route(for: DevicePickerConnectionRoute.electricUnicycle), .eucRide)
        XCTAssertEqual(CutoutAppRoute.route(for: DevicePickerConnectionRoute.vescOnewheel), .vescRide)
        XCTAssertEqual(CutoutAppRoute.route(for: nil), .devicePicker)
    }

}
