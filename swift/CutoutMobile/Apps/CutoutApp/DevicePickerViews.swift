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

    private var renderedScanState: DevicePickerScanState {
        scanState ?? .scanning
    }

    private var sections: DevicePickerSections {
        renderedScanState.sections
    }

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: "setup",
            bottomPadding: 24,
            allowsVerticalScroll: false,
            contentSpacing: 18,
            horizontalPadding: 18
        ) { scale, _ in
            PevScreenTitleBlock(
                title: "Choose device",
                subtitle: "Nearby Bluetooth devices",
                scale: scale
            )

            PevDashboardScanningPill(
                title: connectionStatusText,
                isScanning: renderedScanState.status == .scanning || isConnecting,
                scale: scale
            )
                .padding(.top, 4 * scale)

            if let captureStatusText {
                PevStatusStrip(
                    text: captureStatusText,
                    scale: scale,
                    indicatorColor: PevColors.green,
                    background: PevColors.cardFill,
                    foreground: PevColors.primaryText,
                    cornerRadius: 18
                )
            }

            VStack(alignment: .leading, spacing: 8 * scale) {
                PevDashboardSectionLabel(title: "Device kind for capture")
                TextField("euc nosfet aeon", text: $recordOnlyDeviceKind)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.horizontal, 14 * scale)
                    .frame(minHeight: 46 * scale)
                    .background(PevDashboardCardBackground(cornerRadius: 8 * scale))
                    .accessibilityIdentifier("device-picker.capture-kind")
            }

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 18 * scale) {
                    deviceSection(
                        title: "Supported now",
                        rows: sections.supported,
                        allowsPairing: true,
                        scale: scale
                    )
                    deviceSection(
                        title: "Probe first",
                        rows: sections.probeRecommended,
                        allowsPairing: false,
                        scale: scale
                    )
                    deviceSection(
                        title: "Record only",
                        rows: sections.unsupported,
                        allowsPairing: false,
                        scale: scale
                    )

                    if let manualRow = sections.manual {
                        ManualPickerRow(row: manualRow, scale: scale)
                            .padding(.top, 32 * scale)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .foregroundStyle(.white)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-picker.screen")
    }

    @ViewBuilder
    private func deviceSection(
        title: String,
        rows: [DevicePickerRow],
        allowsPairing: Bool,
        scale: CGFloat
    ) -> some View {
        if !rows.isEmpty {
            PevDashboardSectionLabel(title: title)
                .padding(.top, 8 * scale)
            VStack(spacing: 12 * scale) {
                ForEach(rows) { row in
                    VStack(spacing: 8 * scale) {
                        if allowsPairing {
                            PickerDeviceRow(row: row, scale: scale, action: { pair(row) })
                        } else {
                            PickerDeviceRow(row: row, scale: scale)
                        }
                        captureButton(for: row, scale: scale)
                    }
                }
            }
        }
    }

    private func captureButton(for row: DevicePickerRow, scale: CGFloat) -> some View {
        PevActionButton(
            title: row.captureActionTitle,
            systemImageName: "record.circle",
            scale: scale,
            isEnabled: hasRecordOnlyDeviceKind,
            fillsAvailableWidth: true,
            width: nil,
            height: 44,
            cornerRadius: 8 * scale,
            horizontalPadding: 0,
            iconSpacing: 8 * scale,
            foregroundEnabled: PevColors.yellow,
            foregroundDisabled: PevColors.muted,
            fillEnabled: PevColors.warningFill,
            fillDisabled: PevColors.disabledFill,
            strokeEnabled: PevColors.warningStroke,
            strokeDisabled: PevColors.cardStroke
        ) {
            recordOnly(row, trimmedRecordOnlyDeviceKind)
        }
        .accessibilityIdentifier("device-picker.record.\(row.id)")
    }

    private var isConnecting: Bool {
        switch connectionPhase {
        case .connecting, .discoveringServices, .subscribing:
            true
        default:
            false
        }
    }

    private var connectionStatusText: String {
        isConnecting ? "Connecting…" : renderedScanState.statusText
    }

    private var hasRecordOnlyDeviceKind: Bool {
        !trimmedRecordOnlyDeviceKind.isEmpty
    }

    private var trimmedRecordOnlyDeviceKind: String {
        recordOnlyDeviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
