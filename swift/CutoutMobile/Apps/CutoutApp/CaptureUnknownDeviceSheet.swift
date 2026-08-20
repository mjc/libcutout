import CutoutMobile
import SwiftUI

enum CaptureRecordActionTone: Equatable, Sendable {
    case requiresDeviceKind
    case ready

    static func forDeviceKind(_ deviceKind: String) -> Self {
        deviceKind.isEmpty ? .requiresDeviceKind : .ready
    }

    var isEnabled: Bool {
        self == .ready
    }

    fileprivate var foreground: Color {
        switch self {
        case .requiresDeviceKind: PevColors.brand
        case .ready: PevColors.yellow
        }
    }

    fileprivate var fill: Color {
        switch self {
        case .requiresDeviceKind: PevColors.disabledFill
        case .ready: PevColors.warningFill
        }
    }

    fileprivate var stroke: Color {
        switch self {
        case .requiresDeviceKind: PevColors.cardStroke
        case .ready: PevColors.warningStroke
        }
    }
}

struct CaptureUnknownDeviceSheet: View {
    let sections: DevicePickerSections
    let probe: (DevicePickerRow) -> Bool
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
                ToolbarItem(placement: .cancellationAction) {
                    Button("picker.capture_kind.cancel", role: .cancel) { dismiss() }
                        .accessibilityIdentifier("device-picker.capture-kind.cancel")
                }
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
        let tone =
            row.isProbeRecommended
            ? CaptureRecordActionTone.ready
            : CaptureRecordActionTone.forDeviceKind(trimmedDeviceKind)
        return Button {
            let didStart =
                row.isProbeRecommended
                ? probe(row)
                : recordOnly(row, trimmedDeviceKind)
            if didStart {
                dismiss()
            }
        } label: {
            Label(
                row.captureActionTitle,
                systemImage: row.isProbeRecommended ? "magnifyingglass" : "record.circle"
            )
            .font(.callout.weight(.bold))
            .foregroundStyle(tone.foreground)
            .frame(maxWidth: .infinity, minHeight: 44)
            .background(
                PevDashboardCardBackground(
                    cornerRadius: 8,
                    fill: tone.fill,
                    stroke: tone.stroke
                )
            )
        }
        .buttonStyle(.plain)
        .disabled(!row.isProbeRecommended && !tone.isEnabled)
        .accessibilityLabel(row.captureActionAccessibilityLabel)
        .accessibilityHint(
            row.isProbeRecommended || tone.isEnabled
                ? ""
                : localizedAppText("picker.capture_kind_required_hint")
        )
        .accessibilityIdentifier(
            "device-picker.\(row.isProbeRecommended ? "probe" : "record").\(row.id)"
        )
    }

    private var trimmedDeviceKind: String {
        deviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
