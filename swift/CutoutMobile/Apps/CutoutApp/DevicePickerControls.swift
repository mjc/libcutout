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
            .foregroundStyle(MockupColors.yellow)
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 28 * scale))
        .task(id: isScanning) {
            guard isScanning else { return }
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(260))
                phase = (phase + 1) % 3
            }
        }
    }
}

struct SectionLabel: View {
    let title: String
    let scale: CGFloat

    init(_ title: String, scale: CGFloat) {
        self.title = title
        self.scale = scale
    }

    var body: some View {
        Text(title)
            .font(.system(size: 15 * scale, weight: .semibold))
            .foregroundStyle(MockupColors.muted)
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

            ActionBadge(state: row.state, scale: scale)
        }
        .padding(.horizontal, 18 * scale)
        .frame(height: 92 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 26 * scale))
    }
}

struct RecordOnlyButtonLabel: View {
    let title: String
    let scale: CGFloat
    let isEnabled: Bool

    var body: some View {
        HStack(spacing: 8 * scale) {
            Image(systemName: "record.circle")
                .font(.system(size: 14 * scale, weight: .bold))
            Text(title)
                .font(.system(size: 13 * scale, weight: .bold))
        }
        .foregroundStyle(isEnabled ? MockupColors.yellow : MockupColors.muted)
        .frame(maxWidth: .infinity)
        .frame(height: 36 * scale)
        .background(
            RoundedRectangle(cornerRadius: 8 * scale, style: .continuous)
                .fill(isEnabled ? MockupColors.warningFill : MockupColors.disabledFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 8 * scale, style: .continuous)
                        .stroke(isEnabled ? MockupColors.warningStroke : MockupColors.cardStroke, lineWidth: 1 * scale)
                )
        )
        .contentShape(Rectangle())
    }
}

struct ManualPickerRow: View {
    let row: DevicePickerRow
    let scale: CGFloat

    var body: some View {
        HStack {
            Text(row.title)
                .font(.system(size: 15 * scale, weight: .semibold))
                .foregroundStyle(MockupColors.muted)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
            Spacer()
            ActionBadge(state: row.state, scale: scale)
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 24 * scale))
        .padding(.top, 2 * scale)
    }
}

struct ActionBadge: View {
    let state: DevicePickerRowState
    let scale: CGFloat

    var body: some View {
        Text(state.actionTitle)
            .font(.system(size: 15 * scale, weight: .bold))
            .foregroundStyle(state.isSupported ? .black : MockupColors.muted)
            .frame(width: state.isSupported ? 76 * scale : 64 * scale)
            .frame(height: state.isSupported ? 38 * scale : 30 * scale)
            .background(
                Capsule()
                    .fill(state.isSupported ? MockupColors.yellow : MockupColors.disabledFill)
            )
            .overlay(
                Capsule()
                    .stroke(MockupColors.cardStroke, lineWidth: state.isSupported ? 0 : 1)
            )
    }
}
