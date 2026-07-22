import CutoutMobile

enum EucPackScreen: Hashable {
    case root
    case bmsOverview
    case bmsCellMap6S
    case bmsCellMap40S
    case bmsCellDetail(Int?)
    case bmsUnknownTopology
    case bmsNoData

    init?(screenID: PevScreenID) {
        switch screenID {
        case .bmsOverview: self = .bmsOverview
        case .bmsCellMap6S: self = .bmsCellMap6S
        case .bmsCellMap40S: self = .bmsCellMap40S
        case .bmsCellDetail: self = .bmsCellDetail(nil)
        case .bmsUnknownTopology: self = .bmsUnknownTopology
        case .bmsNoData: self = .bmsNoData
        case .eucRide, .vescRide, .vescDebug: return nil
        }
    }

    var screenID: PevScreenID? {
        switch self {
        case .root: nil
        case .bmsOverview: .bmsOverview
        case .bmsCellMap6S: .bmsCellMap6S
        case .bmsCellMap40S: .bmsCellMap40S
        case .bmsCellDetail: .bmsCellDetail
        case .bmsUnknownTopology: .bmsUnknownTopology
        case .bmsNoData: .bmsNoData
        }
    }
}

enum CutoutAppRoute: Hashable {
    case devicePicker
    case eucRide
    case eucPack(EucPackScreen)
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
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData:
            .eucPack(EucPackScreen(screenID: screenID)!)
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
        case .eucPack:
            .eucPack(.root)
        case .vescRide:
            .vescRide
        }
    }

    static func navigationPath(for route: CutoutAppRoute) -> [CutoutAppRoute] {
        route == .devicePicker ? [] : [route]
    }

    var navigationTabs: [PevScreenTab] {
        switch self {
        case .devicePicker, .capture:
            []
        case .eucRide:
            PevRideTabs.eucRideTabs(selected: .eucRide)
        case .eucPack(let screen):
            PevRideTabs.eucRideTabs(selected: screen.screenID ?? .bmsOverview)
        case .vescRide:
            PevRideTabs.vescRideTabs(selected: .vescRide)
        case .vescDebug:
            PevRideTabs.vescRideTabs(selected: .vescDebug)
        }
    }

    var availableNavigationTabs: [PevScreenTab] {
        navigationTabs.filter { $0.isEnabled && $0.destinationTarget != nil }
    }

    var selectedBmsGroupIndex: Int? {
        guard case let .eucPack(.bmsCellDetail(groupIndex)) = self else { return nil }
        return groupIndex
    }

}
