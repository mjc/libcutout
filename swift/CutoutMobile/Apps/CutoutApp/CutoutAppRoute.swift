import CutoutMobile
import Foundation

enum CutoutAppRoute: Equatable {
    case devicePicker
    case ride
    case liveActivity
    case pack
    case vescRide
    case capture
    case mockup(MockupScreenID)

    var allowsFixtureFallback: Bool {
        if case .mockup = self {
            return true
        }
        return false
    }

    static func initialRoute(
        arguments: [String] = CommandLine.arguments,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> CutoutAppRoute {
        if let id = previewScreenID(arguments: arguments, environment: environment) {
            return .mockup(id)
        }

        return .devicePicker
    }

    static func route(for screenID: MockupScreenID) -> CutoutAppRoute {
        switch screenID {
        case .devicePicker:
            .devicePicker
        case .eucRide:
            .ride
        case .liveActivity:
            .mockup(.liveActivity)
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData, .eucGarage:
            .pack
        case .vescOnewheelRide, .vescDebug:
            .mockup(screenID)
        }
    }

    static func route(for connectionRoute: DevicePickerConnectionRoute?) -> CutoutAppRoute {
        switch connectionRoute {
        case .electricUnicycle?:
            .ride
        case .vescOnewheel?:
            .vescRide
        case nil:
            .devicePicker
        }
    }

    private static func previewScreenID(arguments: [String], environment: [String: String]) -> MockupScreenID? {
        if let id = screenID(after: "--preview-screen", in: arguments) {
            return id
        }
        if let id = screenID(after: "--mockup-screen", in: arguments) {
            return id
        }
        if let value = environment["CUTOUT_PREVIEW_SCREEN"], let id = MockupScreenID(rawValue: value) {
            return id
        }
        if let value = environment["CUTOUT_MOCKUP_SCREEN"], let id = MockupScreenID(rawValue: value) {
            return id
        }
        return nil
    }

    private static func screenID(after flag: String, in arguments: [String]) -> MockupScreenID? {
        guard let index = arguments.firstIndex(of: flag),
              arguments.indices.contains(index + 1) else { return nil }
        return MockupScreenID(rawValue: arguments[index + 1])
    }
}
