import CutoutMobile
import SwiftUI

struct PickerDeviceRow: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let row: DevicePickerRow
    var action: (() -> Void)? = nil

    var body: some View {
        if let action, row.state.isSupported {
            Button(action: action) {
                rowContent
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("device-picker.use.\(row.id)")
            .accessibilityLabel(row.useActionAccessibilityLabel)
            .accessibilityHint(localizedAppText("picker.use_action.hint"))
        } else {
            rowContent
                .accessibilityElement(children: .combine)
        }
    }

    private var rowContent: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 12) {
                    deviceSummary
                    statusPill
                }
            } else {
                HStack(spacing: 14) {
                    deviceSummary
                    Spacer(minLength: 6)
                    statusPill
                }
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 14)
        .frame(minHeight: 92)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 26))
    }

    private var deviceSummary: some View {
        HStack(spacing: 14) {
            DeviceGlyph(row: row)
                .frame(width: 56, height: 56)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 4) {
                Text(row.title)
                    .font(.title3.weight(.bold))
                    .foregroundStyle(row.titleColor)
                Text(row.subtitle)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(row.secondaryTextColor)
                Text(row.detail)
                    .font(.subheadline.weight(.bold))
                    .foregroundStyle(row.secondaryTextColor)
            }
            .layoutPriority(1)
        }
    }

    private var statusPill: some View {
        PevDashboardStatusPill(devicePickerState: row.state)
    }
}

struct ManualPickerRow: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let row: DevicePickerRow

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
    }

    private var title: some View {
        Text(row.title)
            .font(.body.weight(.semibold))
            .foregroundStyle(PevColors.muted)
    }
}
