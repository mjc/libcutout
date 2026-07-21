import CutoutMobile

enum CutoutAppRoute: Hashable {
    case devicePicker
    case eucRide
    case eucPack(PevScreenID)
    case vescRide
    case vescDebug
    case capture

    static func initialRoute() -> CutoutAppRoute {
        .devicePicker
    }

    static func route(for screenID: PevScreenID) -> CutoutAppRoute {
        switch screenID {
        case .eucRide:
            .eucRide
        case .vescRide:
            .vescRide
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData, .eucGarage:
            .eucPack(screenID)
        case .vescDebug:
            .vescDebug
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

    static func route(forNavigationTarget navigationTarget: PevNavigationTarget) -> CutoutAppRoute {
        switch navigationTarget {
        case .screen(let screenID):
            route(for: screenID)
        case .vescRide:
            .vescRide
        }
    }

    static func navigationPath(for route: CutoutAppRoute) -> [CutoutAppRoute] {
        route == .devicePicker ? [] : [route]
    }

    var requiresLiveSession: Bool {
        switch self {
        case .devicePicker, .capture:
            false
        case .eucRide, .eucPack, .vescRide, .vescDebug:
            true
        }
    }

    var navigationTabs: [PevScreenTab] {
        switch self {
        case .devicePicker, .capture:
            []
        case .eucRide:
            PevRideTabs.eucRideTabs(selected: .eucRide)
        case .eucPack(let screenID):
            PevRideTabs.eucRideTabs(selected: screenID)
        case .vescRide:
            PevRideTabs.vescRideTabs(selected: .vescRide)
        case .vescDebug:
            PevRideTabs.vescRideTabs(selected: .vescDebug)
        }
    }

    var availableNavigationTabs: [PevScreenTab] {
        navigationTabs.filter { $0.isEnabled && $0.destinationTarget != nil }
    }

}
