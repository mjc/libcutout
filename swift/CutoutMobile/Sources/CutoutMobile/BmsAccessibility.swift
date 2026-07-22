public extension BmsGroupSnapshot {
    var accessibilityLabel: String {
        guard let label, !label.isEmpty else {
            return pevLocalizedText("bms.accessibility.group", Int64(index))
        }
        return pevLocalizedText("bms.accessibility.group_named", Int64(index), label)
    }

    var accessibilityValue: String {
        var parts = [
            voltage.map {
                pevLocalizedText(
                    "bms.accessibility.voltage",
                    RideUnits.voltageText(millivolts: $0.value, fractionDigits: 3)
                )
            } ?? pevLocalizedText("bms.accessibility.voltage_unavailable"),
            alertLevel.accessibilityValue,
        ]

        if let isBalancing {
            parts.append(
                pevLocalizedText(isBalancing ? "bms.accessibility.balancing" : "bms.accessibility.not_balancing")
            )
        }
        if let detail, !detail.isEmpty {
            parts.append(detail)
        }
        return parts.formatted(.list(type: .and))
    }

    var detailSelectionAccessibilityHint: String {
        pevLocalizedText("bms.accessibility.show_details")
    }
}

public extension BmsAlertLevel {
    var accessibilityValue: String {
        switch self {
        case .nominal:
            pevLocalizedText("bms.accessibility.status.nominal")
        case .warning:
            pevLocalizedText("bms.accessibility.status.warning")
        case .critical:
            pevLocalizedText("bms.accessibility.status.critical")
        case .unknown:
            pevLocalizedText("bms.accessibility.status.unknown")
        }
    }
}
