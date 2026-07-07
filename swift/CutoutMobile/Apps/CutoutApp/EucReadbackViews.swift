import CutoutMobile
import SwiftUI

struct BmsReadbackRows: View {
    let snapshot: BmsSnapshot
    let scale: CGFloat
    var includesPageCursor = true

    private var rows: [SessionDebugRow] {
        includesPageCursor
            ? snapshot.readbackRows
            : snapshot.readbackRows.filter { $0.label != "page" && $0.label != "page verification" }
    }

    var body: some View {
        PevDashboardKeyValueRows(
            rows: rows.enumerated().map { offset, row in
                PevDashboardKeyValueRow(id: "\(offset)-\(row.label)", label: row.label, value: row.value)
            },
            scale: scale,
            fill: MockupColors.cardFill,
            stroke: MockupColors.cardStroke,
            labelColor: MockupColors.muted,
            valueColor: MockupColors.primaryText,
            verticalPadding: 6
        )
    }
}

struct BmsDiagnosticsSection: View {
    let snapshot: BmsSnapshot
    let scale: CGFloat
    @Binding var isExpanded: Bool

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            BmsReadbackRows(snapshot: snapshot, scale: scale, includesPageCursor: false)
                .padding(.top, 8 * scale)
        } label: {
            VStack(alignment: .leading, spacing: 3 * scale) {
                Text("BMS diagnostics")
                    .font(.system(size: 15 * scale, weight: .black))
                    .foregroundStyle(MockupColors.primaryText)
                Text("raw readback, available when we need to debug")
                    .font(.system(size: 11 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
            }
        }
        .tint(MockupColors.muted)
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
                        .foregroundStyle(MockupColors.muted)
                    Spacer()
                    Text(readback.availability.displayText)
                        .font(.system(size: 15 * scale, weight: .black))
                        .foregroundStyle(MockupColors.primaryText)
                }
                .frame(height: 31 * scale)
            } else {
                ForEach(Array(readback.entries.enumerated()), id: \.offset) { offset, entry in
                    VStack(alignment: .leading, spacing: 5 * scale) {
                        HStack {
                            Text("setting \(entry.field.id)")
                                .font(.system(size: 14 * scale, weight: .bold))
                                .foregroundStyle(MockupColors.muted)
                            Spacer()
                            Text("\(entry.field.value)")
                                .font(.system(size: 15 * scale, weight: .black))
                                .monospacedDigit()
                                .foregroundStyle(MockupColors.primaryText)
                        }

                        Text(entry.provenanceText)
                            .font(.system(size: 12 * scale, weight: .semibold))
                            .foregroundStyle(MockupColors.muted)
                            .lineLimit(2)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.vertical, 10 * scale)

                    if offset != readback.entries.indices.last {
                        Rectangle()
                            .fill(MockupColors.cardStroke)
                            .frame(height: 1)
                    }
                }
            }
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 6 * scale)
        .background(CardBackground(cornerRadius: 22 * scale))
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
                    .foregroundStyle(MockupColors.muted)
                Spacer()
                Text(readback.valueText)
                    .font(.system(size: 15 * scale, weight: .black))
                    .monospacedDigit()
                    .foregroundStyle(MockupColors.primaryText)
            }

            Text(readback.detailText)
                .font(.system(size: 12 * scale, weight: .semibold))
                .foregroundStyle(MockupColors.muted)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 16 * scale)
        .background(CardBackground(cornerRadius: 22 * scale))
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
