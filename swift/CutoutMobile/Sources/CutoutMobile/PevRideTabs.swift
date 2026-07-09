import Foundation

public enum PevRideTabs {
    private static let unavailableReason = "LIBCU-423"

    public static func eucRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: selected == nil || selected == .eucRide, destinationTarget: .screen(.eucRide)),
            PevScreenTab(title: "Pack", isSelected: selected == .bmsOverview || selected == .bmsCellMap6S || selected == .bmsCellMap40S || selected == .bmsCellDetail || selected == .bmsUnknownTopology || selected == .bmsNoData, destinationScreenID: .bmsOverview),
            unavailableTab(title: "Map"),
            unavailableTab(title: "Tune"),
        ]
    }

    public static func vescRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: selected == nil, destinationTarget: .vescRide),
            PevScreenTab(title: "Debug", isSelected: selected == .vescDebug, destinationScreenID: .vescDebug),
            unavailableTab(title: "Map"),
            unavailableTab(title: "Logs")
        ]
    }

    private static func unavailableTab(title: String) -> PevScreenTab {
        PevScreenTab(title: title, isSelected: false, disabledReason: unavailableReason)
    }
}
