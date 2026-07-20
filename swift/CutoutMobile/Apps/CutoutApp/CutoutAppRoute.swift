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

    static func navigationPath(for route: CutoutAppRoute) -> [CutoutAppRoute] {
        route == .devicePicker ? [] : [route]
    }

}
