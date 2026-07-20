import CutoutMobile
import SwiftUI

struct PickerDeviceRow: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let row: DevicePickerRow
    let scale: CGFloat
    var action: (() -> Void)? = nil

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 12 * scale) {
                    deviceSummary
                    actionView
                }
            } else {
                HStack(spacing: 14 * scale) {
                    deviceSummary
                    Spacer(minLength: 6 * scale)
                    actionView
                }
            }
        }
        .padding(.horizontal, 18 * scale)
        .padding(.vertical, 14 * scale)
        .frame(minHeight: 92 * scale)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 26 * scale))
    }

    private var deviceSummary: some View {
        HStack(spacing: 14 * scale) {
            DeviceGlyph(row: row)
                .frame(width: 56 * scale, height: 56 * scale)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 4 * scale) {
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
            .accessibilityLabel("Use \(row.title)")
            .accessibilityHint("Connect to this device")
        } else {
            statusPill
        }
    }

    private var statusPill: some View {
        PevDashboardStatusPill(
            title: row.state.actionTitle,
            scale: scale,
            fill: row.state.isSupported ? PevColors.yellow : PevColors.disabledFill,
            foreground: row.state.isSupported ? .black : PevColors.muted,
            stroke: row.state.isSupported ? nil : PevColors.cardStroke,
            width: row.state.isSupported ? 76 : 64,
            horizontalPadding: row.state.isSupported ? 10 : 8,
            height: row.state.isSupported ? 38 : 30,
            fixedHorizontal: true
        )
    }
}

struct ManualPickerRow: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let row: DevicePickerRow
    let scale: CGFloat

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 10 * scale) {
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
        .padding(.horizontal, 22 * scale)
        .padding(.vertical, 10 * scale)
        .frame(minHeight: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
        .padding(.top, 2 * scale)
    }

    private var title: some View {
        Text(row.title)
            .font(.body.weight(.semibold))
            .foregroundStyle(PevColors.muted)
    }

    private var statusPill: some View {
        PevDashboardStatusPill(
            title: row.state.actionTitle,
            scale: scale,
            fill: row.state.isSupported ? PevColors.yellow : PevColors.disabledFill,
            foreground: row.state.isSupported ? .black : PevColors.muted,
            stroke: row.state.isSupported ? nil : PevColors.cardStroke,
            width: row.state.isSupported ? 76 : 64,
            horizontalPadding: row.state.isSupported ? 10 : 8,
            height: row.state.isSupported ? 38 : 30,
            fixedHorizontal: true
        )
    }
}
