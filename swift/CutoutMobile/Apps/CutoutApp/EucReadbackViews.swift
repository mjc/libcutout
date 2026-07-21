import CutoutMobile
import SwiftUI

struct BmsDiagnosticsSection: View {
    let snapshot: BmsSnapshot
    @Binding var isExpanded: Bool

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            PevDashboardKeyValueRows(
                rows: snapshot.readbackRows
                    .filter { $0.label != "page" && $0.label != "page verification" }
                    .map { row in
                        PevDashboardKeyValueRow(id: row.label, label: row.label, value: row.value)
                    },
                fill: PevColors.cardFill,
                stroke: PevColors.cardStroke,
                labelColor: PevColors.muted,
                valueColor: PevColors.primaryText,
                verticalPadding: 6
            )
                .padding(.top, 8)
        } label: {
            VStack(alignment: .leading, spacing: 3) {
                Text("BMS diagnostics")
                    .font(.headline)
                    .foregroundStyle(PevColors.primaryText)
                Text("raw readback, available when we need to debug")
                    .font(.caption.weight(.bold))
                    .foregroundStyle(PevColors.muted)
            }
        }
        .tint(PevColors.muted)
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .background(PevDashboardCardBackground(cornerRadius: 20))
        .accessibilityIdentifier("bms.diagnostics")
    }
}

struct SettingsReadbackRows: View {
    let readback: SettingsReadback

    var body: some View {
        PevDashboardReadbackRows(
            rows: readback.dashboardRows,
            emptyLabel: "settings",
            emptyValue: readback.availability.displayText
        )
    }
}

struct FaultHistoryReadbackRows: View {
    let readback: FaultHistoryReadback

    var body: some View {
        PevDashboardReadbackRows(
            rows: [
                PevDashboardReadbackRow(
                    id: "fault",
                    label: "fault",
                    value: readback.valueText,
                    detail: readback.detailText
                ),
            ]
        )
    }
}

extension SettingsReadback {
    var shouldRender: Bool {
        availability != .available || !entries.isEmpty
    }

    var dashboardRows: [PevDashboardReadbackRow] {
        entries.map { entry in
            PevDashboardReadbackRow(
                id: "setting-\(entry.field.id)",
                label: "setting \(entry.field.id)",
                value: "\(entry.field.value)",
                detail: entry.provenanceText
            )
        }
    }
}

extension FaultHistoryReadback {
    var shouldRender: Bool {
        availability != .available || lastFault != nil || sinceDistance != nil
    }
}

private extension FaultHistoryReadback {
    var valueText: String {
        lastFault.map { "\($0.code.raw.id)=\($0.code.raw.value)" } ?? availability.displayText
    }

    var detailText: String {
        [
            lastFault.map(\.provenanceText),
            sinceDistance.map { "since \($0.value) mm" },
        ]
        .compactMap { $0 }
        .joined(separator: ", ")
    }
}

private extension FaultHistoryEntry {
    var provenanceText: String {
        "\(source.displayText), \(quality.displayText), \(verification.displayText)"
    }
}

private extension SettingsReadbackEntry {
    var provenanceText: String {
        "\(source.displayText), \(quality.displayText), \(verification.displayText)"
    }
}

private extension ReadbackSource {
    var displayText: String {
        switch self {
        case .reported:
            "reported"
        case .calculated:
            "calculated"
        case .estimated:
            "estimated"
        }
    }
}

private extension ReadbackQuality {
    var displayText: String {
        switch self {
        case .known:
            "known"
        case .inferred:
            "inferred"
        }
    }
}

private extension VerificationState {
    var displayText: String {
        switch self {
        case .unverified:
            "unverified"
        case .inferred:
            "inferred"
        case .sourceVerified:
            "source verified"
        case .hardwareVerified:
            "hardware verified"
        case .sourceAndHardwareVerified:
            "source + hardware verified"
        }
    }
}
