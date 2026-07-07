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
        MockupScreenScaffold(
            sectionTitle: "setup",
            bottomPadding: 24,
            allowsVerticalScroll: false,
            contentSpacing: 18,
            horizontalPadding: 18
        ) { scale, _ in
            VStack(alignment: .leading, spacing: 7 * scale) {
                Text("Choose device")
                    .font(.system(size: 34 * scale, weight: .bold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.78)
                Text("Nearby Bluetooth devices")
                    .font(.system(size: 15 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.muted)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }

            ScanStatusPill(
                text: renderedScanState.statusText,
                isScanning: renderedScanState.status == .scanning,
                scale: scale
            )
                .padding(.top, 4 * scale)

            if let captureStatusText {
                CaptureStatusPill(text: captureStatusText, scale: scale)
            }

            VStack(alignment: .leading, spacing: 8 * scale) {
                SectionLabel("Device kind for capture", scale: scale)
                TextField("euc nosfet aeon", text: $recordOnlyDeviceKind)
                    .font(.system(size: 17 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.primaryText)
                    .padding(.horizontal, 14 * scale)
                    .frame(height: 46 * scale)
                    .background(CardBackground(cornerRadius: 8 * scale))
            }

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 18 * scale) {
                    if !sections.supported.isEmpty {
                        SectionLabel("Supported now", scale: scale)
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

                                    Button {
                                        recordOnly(row, trimmedRecordOnlyDeviceKind)
                                    } label: {
                                        RecordOnlyButtonLabel(title: row.captureActionTitle, scale: scale, isEnabled: hasRecordOnlyDeviceKind)
                                    }
                                    .buttonStyle(.plain)
                                    .disabled(!hasRecordOnlyDeviceKind)
                                }
                            }
                        }
                    }

                    if !sections.probeRecommended.isEmpty {
                        SectionLabel("Probe first", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.probeRecommended) { row in
                                VStack(spacing: 8 * scale) {
                                    PickerDeviceRow(row: row, scale: scale)

                                    Button {
                                        recordOnly(row, trimmedRecordOnlyDeviceKind)
                                    } label: {
                                        RecordOnlyButtonLabel(title: row.captureActionTitle, scale: scale, isEnabled: hasRecordOnlyDeviceKind)
                                    }
                                    .buttonStyle(.plain)
                                    .disabled(!hasRecordOnlyDeviceKind)
                                }
                            }
                        }
                    }

                    if !sections.unsupported.isEmpty {
                        SectionLabel("Record only", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.unsupported) { row in
                                VStack(spacing: 8 * scale) {
                                    PickerDeviceRow(row: row, scale: scale)

                                    Button {
                                        recordOnly(row, trimmedRecordOnlyDeviceKind)
                                    } label: {
                                        RecordOnlyButtonLabel(title: row.captureActionTitle, scale: scale, isEnabled: hasRecordOnlyDeviceKind)
                                    }
                                    .buttonStyle(.plain)
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
