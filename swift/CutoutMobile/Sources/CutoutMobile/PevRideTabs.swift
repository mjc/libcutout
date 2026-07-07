import Foundation

public enum PevRideTabs {
    public static func eucRideTabs() -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: true),
            PevScreenTab(title: "Pack", isSelected: false, destinationScreenID: .bmsOverview),
            PevScreenTab(title: "Map", isSelected: false, disabledReason: "LIBCU-423"),
            PevScreenTab(title: "Tune", isSelected: false, disabledReason: "LIBCU-423"),
        ]
    }

    public static func vescRideTabs() -> [PevScreenTab] {
        [
            PevScreenTab(title: "Ride", isSelected: true),
            PevScreenTab(title: "Debug", isSelected: false, destinationScreenID: .vescDebug),
            PevScreenTab(title: "Map", isSelected: false),
            PevScreenTab(title: "Logs", isSelected: false)
        ]
    }
}
