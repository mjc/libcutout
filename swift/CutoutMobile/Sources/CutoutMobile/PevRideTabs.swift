import Foundation

public enum PevRideTabs {
    public static func eucRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: selected == nil || selected == .eucRide, destinationTarget: .screen(.eucRide)),
            PevScreenTab(title: "Pack", isSelected: selected == .eucGarage || selected == .bmsOverview || selected == .bmsCellMap6S || selected == .bmsCellMap40S || selected == .bmsCellDetail || selected == .bmsUnknownTopology || selected == .bmsNoData, destinationScreenID: .bmsOverview),
            unavailableTab(title: "Map", reason: "Map is not available yet."),
            unavailableTab(title: "Tune", reason: "Tune is not available yet."),
        ]
    }

    public static func vescRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: selected == nil || selected == .vescRide, destinationTarget: .vescRide),
            PevScreenTab(title: "Debug", isSelected: selected == .vescDebug, destinationScreenID: .vescDebug),
            unavailableTab(title: "Map", reason: "Map is not available yet."),
            unavailableTab(title: "Logs", reason: "Logs are not available yet.")
        ]
    }

    private static func unavailableTab(title: String, reason: String) -> PevScreenTab {
        PevScreenTab(title: title, isSelected: false, disabledReason: reason)
    }
}
