import CutoutMobile
import SwiftUI

struct DevicePickerView: View {
    let scanState: DevicePickerScanState?
    var connectionPhase: SessionConnectionPhase? = nil
    let pair: (DevicePickerRow) -> Void
    let openSetup: () -> Void

    private var renderedScanState: DevicePickerScanState {
        scanState ?? DevicePickerScanState(status: .idle, rows: [])
    }

    private var sections: DevicePickerSections {
        renderedScanState.sections
    }

    var body: some View {
        ScrollViewReader { proxy in
            PevDashboardScaffold(
                sectionTitle: localizedAppText("picker.section.setup"),
                bottomPadding: 24,
                allowsVerticalScroll: true,
                contentSpacing: 20,
                horizontalPadding: 24,
                showsHeader: false
            ) {
                HStack(alignment: .firstTextBaseline) {
                    PevDashboardBrand()
                    Spacer()
                    Button(action: openSetup) {
                        Image(systemName: "gearshape")
                            .font(.title3)
                            .frame(minWidth: 44, minHeight: 44)
                    }
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.primaryText)
                    .accessibilityIdentifier("device-picker.open-setup")
                    .accessibilityLabel(localizedAppText("picker.section.setup"))
                    .accessibilityHint(localizedAppText("setup.open.hint"))
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("dashboard.top.navigation")

                PevScreenTitleBlock(
                    title: localizedAppText("picker.title"),
                    subtitle: localizedAppText("picker.subtitle.nearby_devices")
                )

                HStack(spacing: 10) {
                    if connectionPresentation.showsActivity {
                        ProgressView()
                            .tint(PevColors.yellow)
                            .accessibilityHidden(true)
                    } else if let symbol = connectionPresentation.symbolName {
                        Image(systemName: symbol)
                            .foregroundStyle(PevColors.yellow)
                            .accessibilityHidden(true)
                    }
                    Text(connectionPresentation.title)
                        .font(.subheadline)
                        .foregroundStyle(PevColors.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .combine)
                .id("device-picker.connection-status")
                .accessibilityIdentifier("device-picker.connection-status")

                VStack(alignment: .leading, spacing: 18) {
                    deviceSection(
                        title: localizedAppText("picker.section.supported_now"),
                        rows: sections.supported + sections.probeRecommended,
                        action: pair
                    )

                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: connectionPhase) {
                proxy.scrollTo("device-picker.connection-status", anchor: .top)
            }
        }
        .foregroundStyle(PevColors.primaryText)
        .buttonStyle(.plain)
        .tint(PevColors.yellow)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-picker.screen")
    }

    @ViewBuilder
    private func deviceSection(
        title: String,
        rows: [DevicePickerRow],
        action: @escaping (DevicePickerRow) -> Void
    ) -> some View {
        if !rows.isEmpty {
            PevDashboardSectionLabel(title: title)
                .padding(.top, 8)
            VStack(spacing: 12) {
                ForEach(rows) { row in
                    PickerDeviceRow(row: row, action: { action(row) })
                }
            }
        }
    }

    private var connectionPresentation: DevicePickerConnectionPresentation {
        DevicePickerConnectionPresentation(scanState: scanState, phase: connectionPhase)
    }
}
