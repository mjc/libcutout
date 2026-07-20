import Foundation

public enum PevRideTabs {
    public static func eucRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: selected == nil || selected == .eucRide, destinationTarget: .screen(.eucRide)),
            PevScreenTab(title: "Pack", isSelected: selected == .bmsOverview || selected == .bmsCellMap6S || selected == .bmsCellMap40S || selected == .bmsCellDetail || selected == .bmsUnknownTopology || selected == .bmsNoData, destinationScreenID: .bmsOverview),
            unavailableTab(title: "Map", reason: "LIBCU-517"),
            unavailableTab(title: "Tune", reason: "LIBCU-518"),
        ]
    }

    public static func vescRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: selected == nil, destinationTarget: .vescRide),
            PevScreenTab(title: "Debug", isSelected: selected == .vescDebug, destinationScreenID: .vescDebug),
            unavailableTab(title: "Map", reason: "LIBCU-517"),
            unavailableTab(title: "Logs", reason: "LIBCU-518")
        ]
    }

    private static func unavailableTab(title: String, reason: String) -> PevScreenTab {
        PevScreenTab(title: title, isSelected: false, disabledReason: reason)
    }
}
