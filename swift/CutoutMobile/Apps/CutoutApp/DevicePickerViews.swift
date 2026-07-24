import CutoutMobile
import SwiftUI

struct DevicePickerView: View {
    let scanState: DevicePickerScanState?
    var connectionPhase: SessionConnectionPhase? = nil
    let captureStatusText: String?
    let isRecordOnlyCapture: Bool
    let hasSavedDevice: Bool
    let pair: (DevicePickerRow) -> Void
    let forgetSavedDevice: () -> Void
    let recordOnly: (DevicePickerRow, String) -> Bool
    @State private var isAdvancedCapturePresented = false

    private var renderedScanState: DevicePickerScanState {
        scanState ?? DevicePickerScanState(status: .idle, rows: [])
    }

    private var sections: DevicePickerSections {
        renderedScanState.sections
    }

    var body: some View {
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
                .accessibilityIdentifier("device-picker.connection-status")

            if hasSavedDevice {
                Button("picker.saved_device.forget", role: .destructive, action: forgetSavedDevice)
                    .frame(minHeight: 44)
                    .accessibilityIdentifier("device-picker.forget-saved-device")
            }

            if let captureStatusText {
                PevStatusStrip(text: captureStatusText)
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
        .foregroundStyle(PevColors.primaryText)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("device-picker.screen")
        .sheet(isPresented: $isAdvancedCapturePresented) {
            CaptureUnknownDeviceSheet(
                sections: sections,
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

private struct CaptureUnknownDeviceSheet: View {
    let sections: DevicePickerSections
    let recordOnly: (DevicePickerRow, String) -> Bool
    @Environment(\.dismiss) private var dismiss
    @State private var deviceKind = ""
    @FocusState private var isDeviceKindFocused: Bool

    var body: some View {
        NavigationStack {
            PevDashboardScaffold(
                sectionTitle: localizedAppText("picker.advanced_capture"),
                bottomPadding: 24,
                allowsVerticalScroll: true,
                contentSpacing: 18,
                horizontalPadding: 24
            ) {
                PevScreenTitleBlock(
                    title: localizedAppText("picker.advanced_capture"),
                    subtitle: localizedAppText("picker.capture_kind.hint")
                )

                VStack(alignment: .leading, spacing: 8) {
                    PevDashboardSectionLabel(title: localizedAppText("picker.capture_kind.label"))
                    TextField(localizedAppText("picker.capture_kind.placeholder"), text: $deviceKind)
                        .font(.body.weight(.semibold))
                        .foregroundStyle(PevColors.primaryText)
                        .padding(.horizontal, 14)
                        .frame(minHeight: 46)
                        .background(PevDashboardCardBackground(cornerRadius: 8))
                        .focused($isDeviceKindFocused)
                        .submitLabel(.done)
                        .onSubmit { isDeviceKindFocused = false }
                        .accessibilityLabel(localizedAppText("picker.capture_kind.label"))
                        .accessibilityHint(localizedAppText("picker.capture_kind.hint"))
                        .accessibilityIdentifier("device-picker.capture-kind")
                }

                captureSection(localizedAppText("picker.section.probe_first"), rows: sections.probeRecommended)
                captureSection(localizedAppText("picker.section.record_only"), rows: sections.unsupported)

                if let manualRow = sections.manual {
                    ManualPickerRow(row: manualRow)
                }
            }
            .accessibilityIdentifier("device-picker.advanced-capture")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("picker.capture_kind.done") { isDeviceKindFocused = false }
                        .accessibilityIdentifier("device-picker.capture-kind.done")
                }
            }
        }
    }

    @ViewBuilder
    private func captureSection(_ title: String, rows: [DevicePickerRow]) -> some View {
        if !rows.isEmpty {
            PevDashboardSectionLabel(title: title)
            VStack(spacing: 12) {
                ForEach(rows) { row in
                    VStack(spacing: 8) {
                        PickerDeviceRow(row: row)
                        captureButton(for: row)
                    }
                }
            }
        }
    }

    private func captureButton(for row: DevicePickerRow) -> some View {
        Button {
            if recordOnly(row, trimmedDeviceKind) {
                dismiss()
            }
        } label: {
            Label(row.captureActionTitle, systemImage: "record.circle")
                .font(.callout.weight(.bold))
                .foregroundStyle(hasDeviceKind ? PevColors.yellow : PevColors.brand)
                .frame(maxWidth: .infinity, minHeight: 44)
                .background(
                    PevDashboardCardBackground(
                        cornerRadius: 8,
                        fill: hasDeviceKind ? PevColors.warningFill : PevColors.disabledFill,
                        stroke: hasDeviceKind ? PevColors.warningStroke : PevColors.cardStroke
                    )
                )
        }
        .buttonStyle(.plain)
        .disabled(!hasDeviceKind)
        .accessibilityLabel(row.captureActionAccessibilityLabel)
        .accessibilityHint(hasDeviceKind ? "" : localizedAppText("picker.capture_kind_required_hint"))
        .accessibilityIdentifier("device-picker.record.\(row.id)")
    }

    private var hasDeviceKind: Bool { !trimmedDeviceKind.isEmpty }

    private var trimmedDeviceKind: String {
        deviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
