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
        arguments: [String] = CommandLine.arguments,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> CutoutAppRoute {
        if let id = configuredScreenID(arguments: arguments, environment: environment) {
            return route(for: id)
        }

        return .devicePicker
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

    private static func configuredScreenID(arguments: [String], environment: [String: String]) -> PevScreenID? {
        if let id = screenID(after: "--preview-screen", in: arguments) {
            return id
        }
        if let value = environment["CUTOUT_PREVIEW_SCREEN"], let id = PevScreenID(rawValue: value) {
            return id
        }
        return nil
    }

    private static func screenID(after flag: String, in arguments: [String]) -> PevScreenID? {
        guard let index = arguments.firstIndex(of: flag),
              arguments.indices.contains(index + 1) else { return nil }
        return PevScreenID(rawValue: arguments[index + 1])
    }
}
