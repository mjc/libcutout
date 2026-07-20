public extension BmsGroupSnapshot {
    var accessibilityLabel: String {
        guard let label, !label.isEmpty else {
            return "Cell group \(index)"
        }
        return "Cell group \(index), \(label)"
    }

    var accessibilityValue: String {
        var parts = [
            voltage.map {
                "\(RideUnits.voltageText(millivolts: $0.value, fractionDigits: 3)) volts"
            } ?? "voltage unavailable",
            alertLevel.accessibilityValue,
        ]

        if let isBalancing {
            parts.append(isBalancing ? "balancing" : "not balancing")
        }
        if let detail, !detail.isEmpty {
            parts.append(detail)
        }
        return parts.joined(separator: ", ")
    }
}

public extension BmsAlertLevel {
    var accessibilityValue: String {
        switch self {
        case .nominal:
            "nominal"
        case .warning:
            "warning"
        case .critical:
            "critical"
        case .unknown:
            "status unknown"
        }
    }
}
