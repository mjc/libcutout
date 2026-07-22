import CutoutMobile
import SwiftUI

struct DevicePickerView: View {
    let scanState: DevicePickerScanState?
    var connectionPhase: SessionConnectionPhase? = nil
    let captureStatusText: String?
    let isRecordOnlyCapture: Bool
    let pair: (DevicePickerRow) -> Void
    let recordOnly: (DevicePickerRow, String) -> Void
    @State private var recordOnlyDeviceKind = ""
    @FocusState private var isCaptureKindFocused: Bool

    private var renderedScanState: DevicePickerScanState {
        scanState ?? DevicePickerScanState(status: .idle, rows: [])
    }

    private var sections: DevicePickerSections {
        renderedScanState.sections
    }

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: "setup",
            bottomPadding: 24,
            allowsVerticalScroll: true,
            contentSpacing: 10,
            horizontalPadding: 24
        ) { _ in
            PevScreenTitleBlock(
                title: "Choose device",
                subtitle: "Nearby Bluetooth devices"
            )

            PevDashboardScanningPill(
                title: connectionPresentation.title,
                isScanning: connectionPresentation.showsActivity,
            )
                .padding(.top, 4)
                .accessibilityIdentifier("device-picker.connection-status")

            if let captureStatusText {
                PevStatusStrip(
                    text: captureStatusText,
                    indicatorColor: PevColors.green,
                    background: PevColors.cardFill,
                    foreground: PevColors.primaryText,
                    cornerRadius: 18
                )
            }

            VStack(alignment: .leading, spacing: 8) {
                PevDashboardSectionLabel(title: "Device kind for capture")
                TextField("Device model", text: $recordOnlyDeviceKind)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.horizontal, 14)
                    .frame(minHeight: 46)
                    .background(PevDashboardCardBackground(cornerRadius: 8))
                    .focused($isCaptureKindFocused)
                    .submitLabel(.done)
                    .onSubmit { isCaptureKindFocused = false }
                    .accessibilityLabel("Device kind for capture")
                    .accessibilityHint("Enter the device family and model, for example euc nosfet aeon")
                    .accessibilityIdentifier("device-picker.capture-kind")
            }

            VStack(alignment: .leading, spacing: 18) {
                deviceSection(
                    title: "Supported now",
                    rows: sections.supported,
                    allowsPairing: true
                )
                deviceSection(
                    title: "Probe first",
                    rows: sections.probeRecommended,
                    allowsPairing: false
                )
                deviceSection(
                    title: "Record only",
                    rows: sections.unsupported,
                    allowsPairing: false
                )

                if let manualRow = sections.manual {
                    ManualPickerRow(row: manualRow)
                        .padding(.top, 32)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .foregroundStyle(PevColors.primaryText)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-picker.screen")
    }

    @ViewBuilder
    private func deviceSection(
        title: String,
        rows: [DevicePickerRow],
        allowsPairing: Bool
    ) -> some View {
        if !rows.isEmpty {
            PevDashboardSectionLabel(title: title)
                .padding(.top, 8)
            VStack(spacing: 12) {
                ForEach(rows) { row in
                    VStack(spacing: 8) {
                        if allowsPairing {
                            PickerDeviceRow(row: row, action: { pair(row) })
                        } else {
                            PickerDeviceRow(row: row)
                        }
                        captureButton(for: row)
                    }
                }
            }
        }
    }

    private func captureButton(for row: DevicePickerRow) -> some View {
        PevActionButton(
            title: row.captureActionTitle,
            systemImageName: "record.circle",
            isEnabled: hasRecordOnlyDeviceKind,
            fillsAvailableWidth: true,
            width: nil,
            height: 44,
            cornerRadius: 8,
            horizontalPadding: 0,
            iconSpacing: 8,
            foregroundEnabled: PevColors.yellow,
            foregroundDisabled: PevColors.primaryText,
            fillEnabled: PevColors.warningFill,
            fillDisabled: PevColors.disabledFill,
            strokeEnabled: PevColors.warningStroke,
            strokeDisabled: PevColors.cardStroke,
            accessibilityLabelText: row.captureActionAccessibilityLabel,
            accessibilityHintText: hasRecordOnlyDeviceKind ? "" : "Enter a device kind above to enable capture",
            accessibilityIdentifierText: "device-picker.record.\(row.id)"
        ) {
            recordOnly(row, trimmedRecordOnlyDeviceKind)
        }
    }

    private var connectionPresentation: DevicePickerConnectionPresentation {
        DevicePickerConnectionPresentation(scanState: scanState, phase: connectionPhase)
    }

    private var hasRecordOnlyDeviceKind: Bool {
        !trimmedRecordOnlyDeviceKind.isEmpty
    }

    private var trimmedRecordOnlyDeviceKind: String {
        recordOnlyDeviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
