import CutoutMobile
import SwiftUI

struct PickerDeviceRow: View {
    let row: DevicePickerRow
    let scale: CGFloat
    var action: (() -> Void)? = nil

    var body: some View {
        HStack(spacing: 14 * scale) {
            DeviceGlyph(row: row)
                .frame(width: 56 * scale, height: 56 * scale)

            VStack(alignment: .leading, spacing: 4 * scale) {
                Text(row.title)
                    .font(.system(size: 20 * scale, weight: .bold))
                    .foregroundStyle(row.titleColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.68)
                Text(row.subtitle)
                    .font(.system(size: 11.5 * scale, weight: .semibold))
                    .foregroundStyle(row.secondaryTextColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                Text(row.detail)
                    .font(.system(size: 12.5 * scale, weight: .bold))
                    .foregroundStyle(row.secondaryTextColor)
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
            }
            .layoutPriority(1)

            Spacer(minLength: 6 * scale)

            actionView
        }
        .padding(.horizontal, 18 * scale)
        .frame(height: 92 * scale)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 26 * scale))
    }

    @ViewBuilder
    private var actionView: some View {
        if let action, row.state.isSupported {
            Button(action: action) {
                statusPill
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("device-picker.use.\(row.id)")
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
            fontSize: 15,
            horizontalPadding: row.state.isSupported ? 10 : 8,
            height: row.state.isSupported ? 38 : 30,
            fixedHorizontal: true
        )
    }
}

struct ManualPickerRow: View {
    let row: DevicePickerRow
    let scale: CGFloat

    var body: some View {
        HStack {
            Text(row.title)
                .font(.system(size: 15 * scale, weight: .semibold))
                .foregroundStyle(PevColors.muted)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
            Spacer()
            PevDashboardStatusPill(
                title: row.state.actionTitle,
                scale: scale,
                fill: row.state.isSupported ? PevColors.yellow : PevColors.disabledFill,
                foreground: row.state.isSupported ? .black : PevColors.muted,
                stroke: row.state.isSupported ? nil : PevColors.cardStroke,
                width: row.state.isSupported ? 76 : 64,
                fontSize: 15,
                horizontalPadding: row.state.isSupported ? 10 : 8,
                height: row.state.isSupported ? 38 : 30,
                fixedHorizontal: true
            )
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 24 * scale))
        .padding(.top, 2 * scale)
    }
}
