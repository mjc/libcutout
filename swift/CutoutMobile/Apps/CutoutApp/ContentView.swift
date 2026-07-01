import CutoutMobile
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: LiveSpeedModel
    @State private var selectedScreenID = MockupScreenID.devicePicker

    private let catalog = MockupScreenCatalog.v2

    var body: some View {
        TabView(selection: $selectedScreenID) {
            ForEach(catalog.screens) { screen in
                MockupScreenContainer(screen: screen, liveSpeed: model.speed.displayValue)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .tag(screen.id)
            }
        }
        .tabViewStyle(.page(indexDisplayMode: .never))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(MockupColors.pageBackground.ignoresSafeArea())
    }
}

private struct MockupScreenContainer: View {
    let screen: MockupScreen
    let liveSpeed: String

    var body: some View {
        switch screen.id {
        case .devicePicker:
            DevicePickerMockupView(screen: screen)
        default:
            GenericMockupView(screen: screen, liveSpeed: liveSpeed)
        }
    }
}

private struct DevicePickerMockupView: View {
    let screen: MockupScreen

    private var supportedRows: [MockupPickerRow] {
        screen.pickerRows.filter { $0.isSupported }
    }

    private var unsupportedRows: [MockupPickerRow] {
        screen.pickerRows.filter { $0.isUnsupported }
    }

    private var manualRow: MockupPickerRow? {
        screen.pickerRows.first { $0.isManual }
    }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(1.0, proxy.size.width / 430.0)
            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 18 * scale) {
                    HStack(alignment: .firstTextBaseline) {
                        Text("CutOut")
                            .font(.system(size: 18 * scale, weight: .bold))
                            .foregroundStyle(MockupColors.yellow)
                        Spacer()
                        Text("setup")
                            .font(.system(size: 15 * scale, weight: .semibold))
                            .foregroundStyle(MockupColors.muted)
                    }
                    .padding(.top, 10 * scale)

                    VStack(alignment: .leading, spacing: 7 * scale) {
                        Text("Pick your device(s)")
                            .font(.system(size: 34 * scale, weight: .bold))
                            .lineLimit(1)
                            .minimumScaleFactor(0.78)
                        Text("Nearby devices that look rideable. Pair supported ones.")
                            .font(.system(size: 15 * scale, weight: .semibold))
                            .foregroundStyle(MockupColors.muted)
                            .lineLimit(2)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    ScanStatusPill(scale: scale)
                        .padding(.top, 4 * scale)

                    SectionLabel("Supported now", scale: scale)
                        .padding(.top, 8 * scale)
                    VStack(spacing: 12 * scale) {
                        ForEach(supportedRows) { row in
                            PickerDeviceRow(row: row, scale: scale)
                        }
                    }

                    SectionLabel("Looks like a PEV, unsupported for launch", scale: scale)
                        .padding(.top, 8 * scale)
                    VStack(spacing: 12 * scale) {
                        ForEach(unsupportedRows) { row in
                            PickerDeviceRow(row: row, scale: scale)
                        }
                    }

                    if let manualRow {
                        ManualPickerRow(row: manualRow, scale: scale)
                    }
                }
                .padding(.horizontal, 18 * scale)
                .padding(.bottom, 24 * scale)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(width: proxy.size.width, height: proxy.size.height, alignment: .top)
            .background(MockupColors.pageBackground)
            .foregroundStyle(.white)
        }
    }
}

private struct ScanStatusPill: View {
    let scale: CGFloat

    var body: some View {
        HStack {
            Text("Scanning Bluetooth")
                .font(.system(size: 18 * scale, weight: .bold))
            Spacer()
            HStack(spacing: 9 * scale) {
                Circle().frame(width: 13 * scale, height: 13 * scale)
                Circle().frame(width: 13 * scale, height: 13 * scale)
                Circle().frame(width: 13 * scale, height: 13 * scale)
            }
            .foregroundStyle(MockupColors.yellow)
        }
        .padding(.horizontal, 22 * scale)
        .frame(height: 64 * scale)
        .frame(maxWidth: .infinity)
        .background(CardBackground(cornerRadius: 28 * scale))
    }
}

private struct SectionLabel: View {
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

private struct PickerDeviceRow: View {
    let row: MockupPickerRow
    let scale: CGFloat

    var body: some View {
        HStack(spacing: 14 * scale) {
            DeviceGlyph(row: row)
                .frame(width: 56 * scale, height: 56 * scale)

            VStack(alignment: .leading, spacing: 4 * scale) {
                Text(row.title)
                    .font(.system(size: 20 * scale, weight: .bold))
                    .foregroundStyle(row.isSupported ? .white : MockupColors.disabledText)
                    .lineLimit(1)
                    .minimumScaleFactor(0.68)
                Text(row.subtitle)
                    .font(.system(size: 11.5 * scale, weight: .semibold))
                    .foregroundStyle(MockupColors.muted)
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                Text(row.detail)
                    .font(.system(size: 12.5 * scale, weight: .bold))
                    .foregroundStyle(MockupColors.muted)
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
        .opacity(row.isSupported ? 1.0 : 0.58)
    }
}

private struct ManualPickerRow: View {
    let row: MockupPickerRow
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

private struct ActionBadge: View {
    let state: MockupPickerRowState
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

private struct DeviceGlyph: View {
    let row: MockupPickerRow

    var body: some View {
        ZStack {
            Circle()
                .fill(row.glyphBackground)
            Circle()
                .stroke(row.glyphColor, lineWidth: 4)
            Image(systemName: row.symbolName)
                .font(.system(size: 26, weight: .bold))
                .foregroundStyle(row.glyphColor)
        }
    }
}

private struct CardBackground: View {
    let cornerRadius: CGFloat

    init(cornerRadius: CGFloat = 22) {
        self.cornerRadius = cornerRadius
    }

    var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(MockupColors.cardFill)
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .stroke(MockupColors.cardStroke, lineWidth: 1)
            )
    }
}

private struct GenericMockupView: View {
    let screen: MockupScreen
    let liveSpeed: String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text(screen.title)
                    .font(.largeTitle.weight(.bold))
                Text(screen.subtitle)
                    .font(.headline)
                    .foregroundStyle(.secondary)
                Text(screen.primaryValue)
                    .font(.system(size: 58, weight: .bold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                Text(screen.secondaryValue)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.secondary)

                if let warning = screen.warning {
                    Text(warning)
                        .font(.headline.weight(.semibold))
                        .foregroundStyle(.orange)
                }

                ForEach(screen.metrics, id: \.label) { metric in
                    HStack {
                        Text(metric.label).foregroundStyle(.secondary)
                        Spacer()
                        Text(metric.value).monospacedDigit()
                    }
                }

                Divider()
                HStack {
                    Text("Live speed").foregroundStyle(.secondary)
                    Spacer()
                    Text("\(liveSpeed) mph").monospacedDigit()
                }
            }
            .padding(24)
        }
        .background(Color.black)
        .foregroundStyle(.white)
    }
}

private extension MockupPickerRow {
    var isSupported: Bool {
        if case .supported = state { true } else { false }
    }

    var isUnsupported: Bool {
        if case .unsupported = state { true } else { false }
    }

    var isManual: Bool {
        if case .manual = state { true } else { false }
    }

    var glyphColor: Color {
        switch title {
        case "NINEBOT-7A31":
            MockupColors.teal
        case "HX Hoverboard":
            MockupColors.brown
        default:
            MockupColors.yellow
        }
    }

    var glyphBackground: Color {
        glyphColor.opacity(isSupported ? 0.12 : 0.16)
    }
}

private enum MockupColors {
    static let pageBackground = Color(red: 0.018, green: 0.020, blue: 0.026)
    static let cardFill = Color(red: 0.045, green: 0.051, blue: 0.073)
    static let cardStroke = Color(red: 0.20, green: 0.23, blue: 0.34)
    static let disabledFill = Color(red: 0.08, green: 0.09, blue: 0.12)
    static let disabledText = Color(red: 0.58, green: 0.59, blue: 0.64)
    static let muted = Color(red: 0.56, green: 0.56, blue: 0.62)
    static let yellow = Color(red: 1.0, green: 0.83, blue: 0.02)
    static let teal = Color(red: 0.19, green: 0.82, blue: 0.86)
    static let brown = Color(red: 0.56, green: 0.36, blue: 0.18)
}

private extension MockupPickerRowState {
    var actionTitle: String {
        switch self {
        case .supported(let action), .unsupported(let action), .manual(let action):
            action
        }
    }

    var isSupported: Bool {
        if case .supported = self { true } else { false }
    }
}


private extension MockupScreen {
    var tabTitle: String {
        switch id {
        case .devicePicker:
            "Picker"
        case .eucRide:
            "EUC"
        case .eucGarage:
            "Pack"
        case .vescOnewheelRide:
            "OW"
        case .vescDebug:
            "VESC"
        }
    }
}
