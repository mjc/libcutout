import CutoutMobile
import SwiftUI

struct BmsDiagnosticsSection: View {
    let snapshot: BmsSnapshot
    @State private var isExpanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            PevDashboardKeyValueRows(
                rows: snapshot.readbackRows
                    .filter { $0.role == .data }
                    .map { row in
                        PevDashboardKeyValueRow(
                            id: row.id,
                            label: row.label,
                            metricValue: row.metricValue
                        )
                    },
                verticalPadding: 6
            )
            .padding(.top, 8)
        } label: {
            VStack(alignment: .leading, spacing: 3) {
                Text(localizedAppText("bms.diagnostics.title"))
                    .font(.headline)
                    .foregroundStyle(PevColors.primaryText)
                Text(localizedAppText("bms.diagnostics.detail"))
                    .font(.caption.weight(.bold))
                    .foregroundStyle(PevColors.muted)
            }
            .accessibilityIdentifier("bms.diagnostics")
        }
        .tint(PevColors.muted)
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .background(PevDashboardCardBackground(cornerRadius: 20))
    }
}
