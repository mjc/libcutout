import CutoutMobile
import SwiftUI

struct PickerDeviceRow: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let row: DevicePickerRow
    var action: (() -> Void)? = nil

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 12) {
                    deviceSummary
                    actionView
                }
            } else {
                HStack(spacing: 14) {
                    deviceSummary
                    Spacer(minLength: 6)
                    actionView
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

    @ViewBuilder
    private var actionView: some View {
        if let action, row.state.isSupported {
            Button(action: action) {
                statusPill
            }
            .buttonStyle(.plain)
            .frame(minWidth: 44, minHeight: 44)
            .accessibilityIdentifier("device-picker.use.\(row.id)")
            .accessibilityLabel(row.useActionAccessibilityLabel)
            .accessibilityHint(localizedAppText("picker.use_action.hint"))
        } else {
            statusPill
        }
    }

    private var statusPill: some View {
        PickerRowStatusPill(state: row.state)
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
                    statusPill
                }
            } else {
                HStack {
                    title
                    Spacer()
                    statusPill
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

    private var statusPill: some View {
        PickerRowStatusPill(state: row.state)
    }
}

private struct PickerRowStatusPill: View {
    let state: DevicePickerRowState

    var body: some View {
        PevDashboardStatusPill(
            title: state.actionTitle,
            fill: state.isSupported ? PevColors.yellow : PevColors.disabledFill,
            foreground: state.isSupported ? .black : PevColors.muted,
            stroke: state.isSupported ? nil : PevColors.cardStroke,
            width: state.isSupported ? 76 : 64,
            horizontalPadding: state.isSupported ? 10 : 8,
            height: state.isSupported ? 38 : 30,
            fixedHorizontal: true
        )
    }
}
