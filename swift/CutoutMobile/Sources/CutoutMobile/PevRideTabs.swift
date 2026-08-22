import Foundation

public enum PevRideTabs {
    public static func eucRideTabs(
        selected: PevScreenID? = nil,
        isTuneSelected: Bool = false
    ) -> [PevScreenTab] {
        [
            PevScreenTab(id: .ride, title: pevLocalizedText("tab.ride"), isSelected: selected == .eucRide || (selected == nil && !isTuneSelected), destinationTarget: .screen(.eucRide)),
            PevScreenTab(id: .pack, title: pevLocalizedText("tab.pack"), isSelected: selected == .bmsOverview || selected == .bmsCellMap6S || selected == .bmsCellMap40S || selected == .bmsCellDetail || selected == .bmsUnknownTopology || selected == .bmsNoData, destinationTarget: .eucPack),
            PevScreenTab(id: .map, title: pevLocalizedText("tab.map"), isSelected: false, destinationTarget: .rideMap),
            PevScreenTab(
                id: .tune,
                title: pevLocalizedText("tab.tune"),
                isSelected: isTuneSelected,
                destinationTarget: .eucTune
            ),
        ]
    }

    public static func vescRideTabs(selected: PevScreenID? = nil) -> [PevScreenTab] {
        [
            PevScreenTab(id: .ride, title: pevLocalizedText("tab.ride"), isSelected: selected == nil || selected == .vescRide, destinationTarget: .vescRide),
            PevScreenTab(id: .debug, title: pevLocalizedText("tab.debug"), isSelected: selected == .vescDebug, destinationScreenID: .vescDebug),
            PevScreenTab(id: .map, title: pevLocalizedText("tab.map"), isSelected: false, destinationTarget: .rideMap),
            unavailableTab(id: .logs, title: pevLocalizedText("tab.logs"), reason: pevLocalizedText("tab.reason.logs_unavailable"))
        ]
    }

    private static func unavailableTab(id: PevScreenTabID, title: String, reason: String) -> PevScreenTab {
        PevScreenTab(id: id, title: title, isSelected: false, disabledReason: reason)
    }
}
