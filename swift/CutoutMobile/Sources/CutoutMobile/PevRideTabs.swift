import Foundation

public enum PevRideTabs {
    public static func eucRideTabs(
        selected: PevScreenID? = nil,
        lightingSelected: Bool = false
    ) -> [PevScreenTab] {
        [
            PevScreenTab(id: .ride, title: pevLocalizedText("tab.ride"), isSelected: !lightingSelected && (selected == nil || selected == .eucRide), destinationTarget: .screen(.eucRide)),
            PevScreenTab(id: .lighting, title: pevLocalizedText("tab.lighting"), isSelected: lightingSelected, destinationTarget: .lighting),
            PevScreenTab(id: .pack, title: pevLocalizedText("tab.pack"), isSelected: selected == .bmsOverview || selected == .bmsCellMap6S || selected == .bmsCellMap40S || selected == .bmsCellDetail || selected == .bmsUnknownTopology || selected == .bmsNoData, destinationTarget: .eucPack),
            unavailableTab(id: .map, title: pevLocalizedText("tab.map"), reason: pevLocalizedText("tab.reason.map_unavailable")),
            unavailableTab(id: .tune, title: pevLocalizedText("tab.tune"), reason: pevLocalizedText("tab.reason.tune_unavailable")),
        ]
    }

    public static func vescRideTabs(
        selected: PevScreenID? = nil,
        lightingSelected: Bool = false
    ) -> [PevScreenTab] {
        [
            PevScreenTab(id: .ride, title: pevLocalizedText("tab.ride"), isSelected: !lightingSelected && (selected == nil || selected == .vescRide), destinationTarget: .vescRide),
            PevScreenTab(id: .lighting, title: pevLocalizedText("tab.lighting"), isSelected: lightingSelected, destinationTarget: .lighting),
            PevScreenTab(id: .debug, title: pevLocalizedText("tab.debug"), isSelected: selected == .vescDebug, destinationScreenID: .vescDebug),
            unavailableTab(id: .map, title: pevLocalizedText("tab.map"), reason: pevLocalizedText("tab.reason.map_unavailable")),
            unavailableTab(id: .logs, title: pevLocalizedText("tab.logs"), reason: pevLocalizedText("tab.reason.logs_unavailable"))
        ]
    }

    private static func unavailableTab(id: PevScreenTabID, title: String, reason: String) -> PevScreenTab {
        PevScreenTab(id: id, title: title, isSelected: false, disabledReason: reason)
    }
}
