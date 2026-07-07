import XCTest
@testable import CutoutApp
import CutoutMobile

final class CutoutAppRouteTests: XCTestCase {
    deinit {}

    func testPreviewArgumentsResolveToMockupRoute() {
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp", "--preview-screen", MockupScreenID.vescDebug.rawValue],
                environment: [:]
            ),
            .mockup(.vescDebug)
        )
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp", "--mockup-screen", MockupScreenID.bmsCellDetail.rawValue],
                environment: [:]
            ),
            .mockup(.bmsCellDetail)
        )
    }

    func testPreviewEnvironmentResolvesToMockupRoute() {
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp"],
                environment: ["CUTOUT_PREVIEW_SCREEN": MockupScreenID.liveActivity.rawValue]
            ),
            .mockup(.liveActivity)
        )
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp"],
                environment: ["CUTOUT_MOCKUP_SCREEN": MockupScreenID.bmsOverview.rawValue]
            ),
            .mockup(.bmsOverview)
        )
    }

    func testPreviewArgumentsWinOverEnvironment() {
        XCTAssertEqual(
            CutoutAppRoute.initialRoute(
                arguments: ["CutoutApp", "--preview-screen", MockupScreenID.vescDebug.rawValue],
                environment: ["CUTOUT_PREVIEW_SCREEN": MockupScreenID.liveActivity.rawValue]
            ),
            .mockup(.vescDebug)
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
        XCTAssertEqual(CutoutAppRoute.route(for: .eucRide), .ride)
        XCTAssertEqual(CutoutAppRoute.route(for: .liveActivity), .mockup(.liveActivity))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsOverview), .pack)
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellMap6S), .pack)
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellMap40S), .pack)
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellDetail), .pack)
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsUnknownTopology), .pack)
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsNoData), .pack)
        XCTAssertEqual(CutoutAppRoute.route(for: .eucGarage), .pack)
        XCTAssertEqual(CutoutAppRoute.route(for: .vescOnewheelRide), .mockup(.vescOnewheelRide))
        XCTAssertEqual(CutoutAppRoute.route(for: .vescDebug), .mockup(.vescDebug))
    }

    func testConnectionRoutesMatchRideSections() {
        XCTAssertEqual(CutoutAppRoute.route(for: DevicePickerConnectionRoute.electricUnicycle), .ride)
        XCTAssertEqual(CutoutAppRoute.route(for: DevicePickerConnectionRoute.vescOnewheel), .vescRide)
        XCTAssertEqual(CutoutAppRoute.route(for: nil), .devicePicker)
    }

    func testBatteryCurrentDetailHidesOnlyAbsentCurrent() {
        XCTAssertEqual(batteryCurrentDetail(nil), "")
        XCTAssertEqual(batteryCurrentDetail(BatteryCurrent(value: 0)), "current 0.0 A")
        XCTAssertEqual(batteryCurrentDetail(BatteryCurrent(value: -2_000)), "current 2.0 A")
    }
}
