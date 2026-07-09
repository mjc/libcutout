import Foundation

public enum PevRideTabs {
    private static let unavailableReason = "LIBCU-423"

    public static func eucRideTabs() -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: true),
            PevScreenTab(title: "Pack", isSelected: false, destinationScreenID: .bmsOverview),
            unavailableTab(title: "Map"),
            unavailableTab(title: "Tune"),
        ]
    }

    public static func vescRideTabs() -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: true),
            PevScreenTab(title: "Debug", isSelected: false, destinationScreenID: .vescDebug),
            unavailableTab(title: "Map"),
            unavailableTab(title: "Logs")
        ]
    }

    private static func unavailableTab(title: String) -> PevScreenTab {
        PevScreenTab(title: title, isSelected: false, disabledReason: unavailableReason)
    }
}
