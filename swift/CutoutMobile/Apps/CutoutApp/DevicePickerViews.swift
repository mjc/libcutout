import CutoutMobile
import SwiftUI

struct DevicePickerView: View {
    let scanState: DevicePickerScanState?
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
                scale: scale,
                titleFontSize: 34,
                subtitleFontSize: 15,
                titleMinimumScaleFactor: 0.78,
                subtitleLineLimit: 2
            )

            PevDashboardScanningPill(
                title: renderedScanState.statusText,
                isScanning: renderedScanState.status == .scanning,
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
                PevDashboardSectionLabel(title: "Device kind for capture", scale: scale)
                TextField("euc nosfet aeon", text: $recordOnlyDeviceKind)
                    .font(.system(size: 17 * scale, weight: .semibold))
                    .foregroundStyle(PevColors.primaryText)
                    .padding(.horizontal, 14 * scale)
                    .frame(height: 46 * scale)
                    .background(PevDashboardCardBackground(cornerRadius: 8 * scale))
            }

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 18 * scale) {
                    if !sections.supported.isEmpty {
                        PevDashboardSectionLabel(title: "Supported now", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.supported) { row in
                                VStack(spacing: 8 * scale) {
                                    Button {
                                        pair(row)
                                    } label: {
                                        PickerDeviceRow(row: row, scale: scale)
                                    }
                                    .buttonStyle(.plain)
                                    .contentShape(Rectangle())

                                    PevActionButton(
                                        title: row.captureActionTitle,
                                        systemImageName: "record.circle",
                                        scale: scale,
                                        isEnabled: hasRecordOnlyDeviceKind,
                                        fillsAvailableWidth: true,
                                        width: nil,
                                        height: 36 * scale,
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
                                    .disabled(!hasRecordOnlyDeviceKind)
                                }
                            }
                        }
                    }

                    if !sections.probeRecommended.isEmpty {
                        PevDashboardSectionLabel(title: "Probe first", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.probeRecommended) { row in
                                VStack(spacing: 8 * scale) {
                                    PickerDeviceRow(row: row, scale: scale)

                                    PevActionButton(
                                        title: row.captureActionTitle,
                                        systemImageName: "record.circle",
                                        scale: scale,
                                        isEnabled: hasRecordOnlyDeviceKind,
                                        fillsAvailableWidth: true,
                                        width: nil,
                                        height: 36 * scale,
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
                                    .disabled(!hasRecordOnlyDeviceKind)
                                }
                            }
                        }
                    }

                    if !sections.unsupported.isEmpty {
                        PevDashboardSectionLabel(title: "Record only", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.unsupported) { row in
                                VStack(spacing: 8 * scale) {
                                    PickerDeviceRow(row: row, scale: scale)

                                    PevActionButton(
                                        title: row.captureActionTitle,
                                        systemImageName: "record.circle",
                                        scale: scale,
                                        isEnabled: hasRecordOnlyDeviceKind,
                                        fillsAvailableWidth: true,
                                        width: nil,
                                        height: 36 * scale,
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
                                    .disabled(!hasRecordOnlyDeviceKind)
                                }
                            }
                        }
                    }

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
    }

    private var hasRecordOnlyDeviceKind: Bool {
        !trimmedRecordOnlyDeviceKind.isEmpty
    }

    private var trimmedRecordOnlyDeviceKind: String {
        recordOnlyDeviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
