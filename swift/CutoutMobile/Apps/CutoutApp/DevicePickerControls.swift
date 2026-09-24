import CutoutMobile
import SwiftUI

struct PickerDeviceRow: View {
    let row: DevicePickerRow
    var action: (() -> Void)? = nil
    @State private var showsDetails = false

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .top, spacing: 14) {
                deviceSummary
                if action != nil {
                    Button { showsDetails = true } label: {
                        Image(systemName: "info.circle")
                            .font(.title3)
                            .foregroundStyle(PevColors.muted)
                            .frame(minWidth: 44, minHeight: 44)
                    }
                    .accessibilityLabel(localizedAppText("picker.details_for_device", row.title))
                    .accessibilityIdentifier("device-picker.details.\(row.id)")
                }
            }
            if let action, row.state.isSupported || row.isProbeRecommended {
                Button(action: action) {
                    Text(localizedAppText("picker.connect"))
                        .font(.headline)
                        .foregroundStyle(.black)
                        .frame(maxWidth: .infinity, minHeight: 46)
                        .background(PevColors.yellow, in: RoundedRectangle(cornerRadius: 14))
                }
                .accessibilityIdentifier("device-picker.use.\(row.id)")
                .accessibilityLabel(localizedAppText("picker.connect_device", row.title))
                .accessibilityValue(row.secondaryIdentity ?? String(row.id.suffix(4).uppercased()))
                .accessibilityHint(localizedAppText("picker.use_action.hint"))
            } else {
                Text(row.subtitle)
                    .font(.subheadline)
                    .foregroundStyle(row.secondaryTextColor)
                Text(row.detail)
                    .font(.subheadline)
                    .foregroundStyle(row.secondaryTextColor)
                PevDashboardStatusPill(devicePickerState: row.state)
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 24))
        .buttonStyle(.plain)
        .sheet(isPresented: $showsDetails) {
            PickerDeviceDetails(row: row)
        }
    }

    private var deviceSummary: some View {
        HStack(spacing: 14) {
            DeviceGlyph(row: row)
                .frame(width: 56, height: 56)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 4) {
                Text(row.title)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(row.titleColor)
                if let identity = row.secondaryIdentity {
                    Text(identity)
                        .font(.subheadline)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("device-picker.identity.\(row.id)")
                }
            }
            .fixedSize(horizontal: false, vertical: true)
            .layoutPriority(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

private struct PickerDeviceDetails: View {
    let row: DevicePickerRow
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    if let name = row.advertisedName {
                        LabeledContent(localizedAppText("picker.advertised_name"), value: name)
                    }
                    LabeledContent(localizedAppText("picker.device_identifier"), value: row.id)
                }
                Section(localizedAppText("picker.detection_details")) {
                    Text(row.subtitle)
                    Text(row.detail)
                }
            }
            .textSelection(.enabled)
            .navigationTitle(row.title)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(localizedAppText("picker.capture_kind.done")) { dismiss() }
                        .accessibilityIdentifier("device-picker.device-details.done")
                }
            }
        }
        .tint(PevColors.yellow)
        .accessibilityIdentifier("device-picker.device-details")
    }
}

struct ManualPickerRow: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let row: DevicePickerRow
    var accessibilityLabelText: String { row.title }
    var accessibilityValueText: String { row.state.actionTitle }

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 10) {
                    title
                    PevDashboardStatusPill(devicePickerState: row.state)
                }
            } else {
                HStack {
                    title
                    Spacer()
                    PevDashboardStatusPill(devicePickerState: row.state)
                }
            }
        }
        .padding(.horizontal, 22)
        .padding(.vertical, 10)
        .frame(minHeight: 64)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 24))
        .padding(.top, 2)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityLabelText)
        .accessibilityValue(accessibilityValueText)
    }

    private var title: some View {
        Text(row.title)
            .font(.body.weight(.semibold))
            .foregroundStyle(PevColors.muted)
    }
}
