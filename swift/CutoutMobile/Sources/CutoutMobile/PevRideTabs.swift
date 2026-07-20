import Foundation

public enum PevRideTabs {
    public static func eucRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(id: .ride, title: "Ride", isSelected: selected == nil || selected == .eucRide, destinationTarget: .screen(.eucRide)),
            PevScreenTab(id: .pack, title: "Pack", isSelected: selected == .eucGarage || selected == .bmsOverview || selected == .bmsCellMap6S || selected == .bmsCellMap40S || selected == .bmsCellDetail || selected == .bmsUnknownTopology || selected == .bmsNoData, destinationScreenID: .bmsOverview),
            unavailableTab(id: .map, title: "Map", reason: "Map is not available yet."),
            unavailableTab(id: .tune, title: "Tune", reason: "Tune is not available yet."),
        ]
    }

    public static func vescRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(id: .ride, title: "Ride", isSelected: selected == nil || selected == .vescRide, destinationTarget: .vescRide),
            PevScreenTab(id: .debug, title: "Debug", isSelected: selected == .vescDebug, destinationScreenID: .vescDebug),
            unavailableTab(id: .map, title: "Map", reason: "Map is not available yet."),
            unavailableTab(id: .logs, title: "Logs", reason: "Logs are not available yet.")
        ]
    }

    private static func unavailableTab(id: PevScreenTabID, title: String, reason: String) -> PevScreenTab {
        PevScreenTab(id: id, title: title, isSelected: false, disabledReason: reason)
    }
}
