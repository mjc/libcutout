import CutoutMobile
import SwiftUI

struct DevicePickerView: View {
    let scanState: DevicePickerScanState?
    var connectionPhase: SessionConnectionPhase? = nil
    let captureStatusText: String?
    let hasSavedDevice: Bool
    let pair: (DevicePickerRow) -> Void
    let forgetSavedDevice: () -> Void
    let probe: (DevicePickerRow) -> Bool
    let recordOnly: (DevicePickerRow, String) -> Bool
    @State private var isAdvancedCapturePresented = false

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
                contentSpacing: 10,
                horizontalPadding: 24
            ) {
                PevScreenTitleBlock(
                    title: localizedAppText("picker.title"),
                    subtitle: localizedAppText("picker.subtitle.nearby_devices")
                )

                PevDashboardScanningPill(
                    title: connectionPresentation.title,
                    isScanning: connectionPresentation.showsActivity,
                    symbolName: connectionPresentation.symbolName
                )
                .padding(.top, 4)
                .id("device-picker.connection-status")
                .accessibilityIdentifier("device-picker.connection-status")

                if hasSavedDevice {
                    Button("picker.saved_device.forget", role: .destructive, action: forgetSavedDevice)
                        .frame(minHeight: 44)
                        .accessibilityIdentifier("device-picker.forget-saved-device")
                }

                if let captureStatusText {
                    PevStatusStrip(
                        text: captureStatusText,
                        accessibilityIdentifier: "device-picker.capture-status"
                    )
                }

                VStack(alignment: .leading, spacing: 18) {
                    deviceSection(
                        title: localizedAppText("picker.section.supported_now"),
                        rows: sections.supported
                    )

                    Button("picker.advanced_capture") { isAdvancedCapturePresented = true }
                        .frame(maxWidth: .infinity, minHeight: 44)
                        .accessibilityIdentifier("device-picker.open-advanced-capture")
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: connectionPhase) {
                proxy.scrollTo("device-picker.connection-status", anchor: .top)
            }
        }
        .foregroundStyle(PevColors.primaryText)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-picker.screen")
        .sheet(isPresented: $isAdvancedCapturePresented) {
            CaptureUnknownDeviceSheet(
                sections: sections,
                probe: probe,
                recordOnly: recordOnly
            )
        }
    }

    @ViewBuilder
    private func deviceSection(
        title: String,
        rows: [DevicePickerRow]
    ) -> some View {
        if !rows.isEmpty {
            PevDashboardSectionLabel(title: title)
                .padding(.top, 8)
            VStack(spacing: 12) {
                ForEach(rows) { row in
                    PickerDeviceRow(row: row, action: { pair(row) })
                }
            }
        }
    }

    private var connectionPresentation: DevicePickerConnectionPresentation {
        DevicePickerConnectionPresentation(scanState: scanState, phase: connectionPhase)
    }
}
