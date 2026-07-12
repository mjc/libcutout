import CutoutMobile
import Foundation

enum CutoutAppRoute: Equatable {
    case devicePicker
    case eucRide
    case eucMap
    case eucTune
    case liveActivity
    case eucPack(PevScreenID)
    case vescRide
    case vescDebug
    case vescMap
    case vescLogs
    case capture

    static func initialRoute(
        arguments _: [String] = CommandLine.arguments,
        environment _: [String: String] = ProcessInfo.processInfo.environment
    ) -> CutoutAppRoute {
        .devicePicker
    }

    static func route(for screenID: PevScreenID) -> CutoutAppRoute {
        switch screenID {
        case .devicePicker:
            .devicePicker
        case .eucRide:
            .eucRide
        case .eucMap:
            .eucMap
        case .eucTune:
            .eucTune
        case .liveActivity:
            .liveActivity
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData, .eucGarage:
            .eucPack(screenID)
        case .vescDebug:
            .vescDebug
        case .vescMap:
            .vescMap
        case .vescLogs:
            .vescLogs
        }
    }

    static func route(for connectionRoute: DevicePickerConnectionRoute?) -> CutoutAppRoute {
        switch connectionRoute {
        case .electricUnicycle?:
            .eucRide
        case .vescOnewheel?:
            .vescRide
        case nil:
            .devicePicker
        }
    }

}
