import CutoutMobile
import SwiftUI

struct BmsDiagnosticsSection: View {
    let snapshot: BmsSnapshot
    let scale: CGFloat
    @Binding var isExpanded: Bool

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            PevDashboardKeyValueRows(
                rows: snapshot.readbackRows
                    .filter { $0.label != "page" && $0.label != "page verification" }
                    .enumerated()
                    .map { offset, row in
                        PevDashboardKeyValueRow(id: "\(offset)-\(row.label)", label: row.label, value: row.value)
                    },
                scale: scale,
                fill: PevColors.cardFill,
                stroke: PevColors.cardStroke,
                labelColor: PevColors.muted,
                valueColor: PevColors.primaryText,
                verticalPadding: 6
            )
                .padding(.top, 8 * scale)
        } label: {
            VStack(alignment: .leading, spacing: 3 * scale) {
                Text("BMS diagnostics")
                    .font(.system(size: 15 * scale, weight: .black))
                    .foregroundStyle(PevColors.primaryText)
                Text("raw readback, available when we need to debug")
                    .font(.system(size: 11 * scale, weight: .bold))
                    .foregroundStyle(PevColors.muted)
            }
        }
        .tint(PevColors.muted)
        .padding(.horizontal, 16 * scale)
        .padding(.vertical, 14 * scale)
        .background(PevDashboardCardBackground(cornerRadius: 20 * scale))
    }
}

struct SettingsReadbackRows: View {
    let readback: SettingsReadback
    let scale: CGFloat

    var body: some View {
        VStack(spacing: 0) {
            if readback.entries.isEmpty {
                HStack {
                    Text("settings")
                        .font(.system(size: 14 * scale, weight: .bold))
                        .foregroundStyle(PevColors.muted)
                    Spacer()
                    Text(readback.availability.displayText)
                        .font(.system(size: 15 * scale, weight: .black))
                        .foregroundStyle(PevColors.primaryText)
                }
                .frame(height: 31 * scale)
            } else {
                ForEach(Array(readback.entries.enumerated()), id: \.offset) { offset, entry in
                    VStack(alignment: .leading, spacing: 5 * scale) {
                        HStack {
                            Text("setting \(entry.field.id)")
                                .font(.system(size: 14 * scale, weight: .bold))
                                .foregroundStyle(PevColors.muted)
                            Spacer()
                            Text("\(entry.field.value)")
                                .font(.system(size: 15 * scale, weight: .black))
                                .monospacedDigit()
                                .foregroundStyle(PevColors.primaryText)
                        }

                        Text(entry.provenanceText)
                            .font(.system(size: 12 * scale, weight: .semibold))
                            .foregroundStyle(PevColors.muted)
                            .lineLimit(2)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.vertical, 10 * scale)

                    if offset != readback.entries.indices.last {
                        Rectangle()
                            .fill(PevColors.cardStroke)
                            .frame(height: 1)
                    }
                }
            }
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 6 * scale)
        .background(PevDashboardCardBackground(cornerRadius: 22 * scale))
    }
}

struct FaultHistoryReadbackRows: View {
    let readback: FaultHistoryReadback
    let scale: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 5 * scale) {
            HStack {
                Text("fault")
                    .font(.system(size: 14 * scale, weight: .bold))
                    .foregroundStyle(PevColors.muted)
                Spacer()
                Text(readback.valueText)
                    .font(.system(size: 15 * scale, weight: .black))
                    .monospacedDigit()
                    .foregroundStyle(PevColors.primaryText)
            }

            Text(readback.detailText)
                .font(.system(size: 12 * scale, weight: .semibold))
                .foregroundStyle(PevColors.muted)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 16 * scale)
        .background(PevDashboardCardBackground(cornerRadius: 22 * scale))
    }
}

extension SettingsReadback {
    var shouldRender: Bool {
        availability != .available || !entries.isEmpty
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
