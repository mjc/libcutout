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

    func hasAvailableSelectedGroup(in groupIndices: [Int]?) -> Bool {
        guard case let .bmsCellDetail(selectedGroupIndex?) = self,
              let groupIndices else {
            return true
        }
        return groupIndices.contains(selectedGroupIndex)
    }
}

enum CutoutAppRoute: Hashable {
    case devicePicker
    case eucRide
    case lighting(LightingRideContext)
    case eucPack(EucPackScreen)
    case eucTune
    case vescRide
    case vescDebug
    case capture
    case camera
    case rideMap
    case rideMapDetail(rideID: String)

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
        route(forNavigationTarget: navigationTarget, from: nil)
    }

    static func route(
        forNavigationTarget navigationTarget: PevNavigationTarget,
        from source: CutoutAppRoute?
    ) -> CutoutAppRoute {
        switch navigationTarget {
        case .screen(let screenID):
            route(for: screenID)
        case .eucPack:
            .eucPack(.root)
        case .eucTune:
            .eucTune
        case .vescRide:
            .vescRide
        case .rideMap:
            .rideMap
        case .lighting:
            .lighting(source?.lightingContext ?? .euc)
        }
    }

    static func navigationPath(for route: CutoutAppRoute) -> [CutoutAppRoute] {
        switch route {
        case .devicePicker:
            []
        case let .rideMapDetail(rideID):
            [.rideMap, .rideMapDetail(rideID: rideID)]
        default:
            [route]
        }
    }

    static func navigationPath(openingCameraFrom route: CutoutAppRoute) -> [CutoutAppRoute] {
        guard route != .camera else { return [.camera] }
        return navigationPath(for: route) + [.camera]
    }

    var preservesNavigationOnConnectionLoss: Bool {
        switch self {
        case .rideMap, .rideMapDetail, .lighting:
            true
        default:
            false
        }
    }

    private var routeTabs: [PevScreenTab] {
        switch self {
        case .devicePicker, .capture, .camera, .rideMap, .rideMapDetail:
            []
        case .eucRide:
            PevRideTabs.eucRideTabs(selected: .eucRide)
        case .lighting(let context):
            switch context {
            case .euc:
                PevRideTabs.eucRideTabs(lightingSelected: true)
            case .vesc:
                PevRideTabs.vescRideTabs(lightingSelected: true)
            }
        case .eucPack(let screen):
            PevRideTabs.eucRideTabs(selected: screen.screenID ?? .bmsOverview)
        case .eucTune:
            PevRideTabs.eucRideTabs(isTuneSelected: true)
        case .vescRide:
            PevRideTabs.vescRideTabs(selected: .vescRide)
        case .vescDebug:
            PevRideTabs.vescRideTabs(selected: .vescDebug)
        }
    }

    func navigationTabs(for connectionRoute: DevicePickerConnectionRoute?) -> [PevScreenTab] {
        switch self {
        case .rideMap, .rideMapDetail:
            guard let connectionRoute else {
                return [PevScreenTab(
                    id: .map,
                    title: pevLocalizedText("tab.map"),
                    isSelected: true,
                    destinationTarget: .rideMap
                )]
            }
            let tabs = switch connectionRoute {
            case .electricUnicycle: PevRideTabs.eucRideTabs()
            case .vescOnewheel: PevRideTabs.vescRideTabs()
            }
            return tabs.map { tab in
                PevScreenTab(
                    id: tab.id,
                    title: tab.title,
                    isSelected: tab.id == .map,
                    destinationScreenID: tab.destinationScreenID,
                    destinationTarget: tab.destinationTarget,
                    disabledReason: tab.disabledReason
                )
            }
        default:
            return routeTabs
        }
    }

    func availableNavigationTabs(for connectionRoute: DevicePickerConnectionRoute?) -> [PevScreenTab] {
        navigationTabs(for: connectionRoute).filter { $0.isEnabled && $0.destinationTarget != nil }
    }

    func destination(for tab: PevScreenTab, connectionRoute: DevicePickerConnectionRoute? = nil) -> CutoutAppRoute? {
        guard let target = tab.destinationTarget else { return nil }
        if tab.id == .pack, case .eucPack = self { return self }
        return destination(forNavigationTarget: target, connectionRoute: connectionRoute)
    }

    func destination(
        forNavigationTarget target: PevNavigationTarget,
        connectionRoute: DevicePickerConnectionRoute? = nil
    ) -> CutoutAppRoute {
        let source = lightingContext == nil ? Self.route(for: connectionRoute) : self
        return Self.route(forNavigationTarget: target, from: source)
    }

    private var lightingContext: LightingRideContext? {
        switch self {
        case .eucRide, .eucPack, .lighting(.euc): .euc
        case .vescRide, .vescDebug, .lighting(.vesc): .vesc
        default: nil
        }
    }

    var selectedBmsGroupIndex: Int? {
        guard case let .eucPack(.bmsCellDetail(groupIndex)) = self else { return nil }
        return groupIndex
    }

}

enum LightingRideContext: Hashable {
    case euc
    case vesc
}
