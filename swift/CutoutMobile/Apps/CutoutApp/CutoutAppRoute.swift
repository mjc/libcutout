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
        guard case .bmsCellDetail(let selectedGroupIndex?) = self,
            let groupIndices
        else {
            return true
        }
        return groupIndices.contains(selectedGroupIndex)
    }
}

enum CutoutAppRoute: Hashable {
    case devicePicker
    case more
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
        case .devicePicker:
            .devicePicker
        case .more:
            .more
        case .camera:
            .camera
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
        case .rideMapDetail(let rideID):
            [.rideMap, .rideMapDetail(rideID: rideID)]
        case .camera, .lighting, .eucPack, .vescDebug:
            [.more, route]
        default:
            [route]
        }
    }

    /// The first primary section selects a tab; only its children belong to the stack.
    static func navigationRoot(for navigationPath: [Self]) -> Self {
        switch navigationPath.first {
        case .eucRide?, .vescRide?, .eucTune?, .rideMap?, .more?:
            navigationPath[0]
        default:
            .devicePicker
        }
    }

    static func stackNavigationPath(for navigationPath: [Self], root: Self? = nil) -> [Self] {
        if let root, navigationRoot(for: navigationPath) != root { return [] }
        return navigationRoot(for: navigationPath) == .devicePicker
            ? navigationPath
            : Array(navigationPath.dropFirst())
    }

    static func replacingStackNavigationPath(_ stackPath: [Self], in navigationPath: [Self], root: Self? = nil)
        -> [Self]
    {
        if let root, navigationRoot(for: navigationPath) != root { return navigationPath }
        return Self.navigationPath(for: navigationRoot(for: navigationPath)) + stackPath
    }

    var preservesNavigationOnConnectionLoss: Bool {
        switch self {
        case .capture, .camera, .rideMap, .rideMapDetail, .lighting, .more:
            true
        default:
            false
        }
    }

    private var routeTabs: [PevScreenTab] {
        switch self {
        case .devicePicker, .capture, .camera, .rideMap, .rideMapDetail, .more:
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
        let cameraTab = PevScreenTab(
            id: .camera, title: localizedAppText("navigation.section.camera"),
            isSelected: self == .camera, destinationTarget: .camera
        )
        if self == .devicePicker
            || (connectionRoute == nil
                && (self == .camera || self == .more || self == .rideMap || isRideMapDetail || isMoreDestination))
        {
            return [
                PevScreenTab(
                    id: .devices, title: localizedAppText("navigation.tab.devices"),
                    isSelected: self == .devicePicker, destinationTarget: .devicePicker
                ),
                PevScreenTab(
                    id: .map, title: pevLocalizedText("tab.map"),
                    isSelected: self == .rideMap || isRideMapDetail, destinationTarget: .rideMap
                ),
                cameraTab,
                PevScreenTab(
                    id: .lighting, title: localizedAppText("navigation.section.lighting"),
                    isSelected: self == .lighting(.euc) || self == .lighting(.vesc), destinationTarget: .lighting
                ),
            ]
        }
        let tabs: [PevScreenTab]
        switch self {
        case .camera, .rideMap, .rideMapDetail, .more:
            guard let connectionRoute else {
                return [
                    PevScreenTab(
                        id: .map,
                        title: pevLocalizedText("tab.map"),
                        isSelected: true,
                        destinationTarget: .rideMap
                    )
                ]
            }
            let connectedTabs =
                switch connectionRoute {
                case .electricUnicycle: PevRideTabs.eucRideTabs()
                case .vescOnewheel: PevRideTabs.vescRideTabs()
                }
            tabs = connectedTabs.map { tab in
                PevScreenTab(
                    id: tab.id,
                    title: tab.title,
                    isSelected: (self == .rideMap || isRideMapDetail) && tab.id == .map,
                    destinationScreenID: tab.destinationScreenID,
                    destinationTarget: tab.destinationTarget,
                    disabledReason: tab.disabledReason
                )
            }
        default:
            tabs = routeTabs
        }
        guard !tabs.isEmpty else { return tabs }
        var result = tabs
        result.insert(
            cameraTab,
            at: result.firstIndex(where: { $0.id == .map }) ?? result.endIndex
        )
        return result
    }

    func availableNavigationTabs(for connectionRoute: DevicePickerConnectionRoute?) -> [PevScreenTab] {
        navigationTabs(for: connectionRoute).filter { $0.isEnabled && $0.destinationTarget != nil }
    }

    func primaryNavigationTabs(for connectionRoute: DevicePickerConnectionRoute?) -> [PevScreenTab] {
        let destinations = availableNavigationTabs(for: connectionRoute)
        let primary = destinations.filter { $0.id == .ride || $0.id == .map || $0.id == .tune || $0.id == .devices }
        let secondary = moreNavigationTabs(for: connectionRoute)
        guard !secondary.isEmpty else { return primary }
        return primary + [
            PevScreenTab(
                id: .more, title: localizedAppText("navigation.section.more"),
                isSelected: self == .more || secondary.contains(where: \.isSelected),
                destinationTarget: .more
            )
        ]
    }

    func moreNavigationTabs(for connectionRoute: DevicePickerConnectionRoute?) -> [PevScreenTab] {
        availableNavigationTabs(for: connectionRoute).filter {
            $0.id != .ride && $0.id != .map && $0.id != .tune && $0.id != .devices
        }
    }

    var isMoreDestination: Bool {
        switch self {
        case .camera, .lighting, .eucPack, .vescDebug: true
        default: false
        }
    }

    private var isRideMapDetail: Bool {
        if case .rideMapDetail = self { return true }
        return false
    }

    func destination(for tab: PevScreenTab, connectionRoute: DevicePickerConnectionRoute? = nil) -> CutoutAppRoute? {
        guard let target = tab.destinationTarget else { return nil }
        if tab.id == .more, isMoreDestination { return self }
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
        guard case .eucPack(.bmsCellDetail(let groupIndex)) = self else { return nil }
        return groupIndex
    }

}

enum LightingRideContext: Hashable {
    case euc
    case vesc
}
