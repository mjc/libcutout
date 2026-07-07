import CutoutMobile
import SwiftUI

struct ScanStatusPill: View {
    let text: String
    let isScanning: Bool
    let scale: CGFloat
    @State private var phase = 0

    var body: some View {
        HStack {
            Text(text)
                .font(.system(size: 18 * scale, weight: .bold))
            Spacer()
            HStack(spacing: 9 * scale) {
                ForEach(0..<3, id: \.self) { index in
                    Circle()
                        .frame(width: 13 * scale, height: 13 * scale)
                        .opacity(!isScanning || index == phase ? 1 : 0.32)
                }
            }
            .foregroundStyle(PevColors.yellow)
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 28 * scale))
        .task(id: isScanning) {
            guard isScanning else { return }
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(260))
                phase = (phase + 1) % 3
            }
        }
    }
}

struct PickerDeviceRow: View {
    let row: DevicePickerRow
    let scale: CGFloat

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
        .padding(.horizontal, 18 * scale)
        .frame(height: 92 * scale)
        .frame(maxWidth: .infinity)
        .background(PevDashboardCardBackground(cornerRadius: 26 * scale))
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
