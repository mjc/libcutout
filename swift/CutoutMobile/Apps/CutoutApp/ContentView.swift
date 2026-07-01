import CutoutMobile
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: LiveSpeedModel
    @State private var selectedScreenID = MockupScreenID.devicePicker

    private let catalog = MockupScreenCatalog.v2

    var body: some View {
        ZStack {
            MockupColors.pageBackground
                .ignoresSafeArea()

            TabView(selection: $selectedScreenID) {
                ForEach(catalog.screens) { screen in
                    MockupScreenContainer(
                        screen: screen,
                        liveSpeed: model.speed.displayValue,
                        devicePickerScanState: model.devicePickerScanState
                    )
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                        .tag(screen.id)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(MockupColors.pageBackground.ignoresSafeArea())
    }
}

private struct MockupScreenContainer: View {
    let screen: MockupScreen
    let liveSpeed: String
    let devicePickerScanState: DevicePickerScanState?

    var body: some View {
        switch screen.id {
        case .devicePicker:
            DevicePickerMockupView(screen: screen, scanState: devicePickerScanState)
        default:
            GenericMockupView(screen: screen, liveSpeed: liveSpeed)
        }
    }
}

private struct DevicePickerMockupView: View {
    let screen: MockupScreen
    let scanState: DevicePickerScanState?

    private var renderedScanState: DevicePickerScanState {
        if let scanState {
            return scanState
        }
        return DevicePickerScanState(status: .scanning, rows: screen.pickerRows)
    }

    private var sections: MockupPickerSections {
        renderedScanState.sections
    }

    var body: some View {
        GeometryReader { proxy in
            let scale = min(proxy.size.width / 390.0, proxy.size.height / 844.0)
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

                    ScanStatusPill(text: renderedScanState.statusText, scale: scale)
                        .padding(.top, 4 * scale)

                    if !sections.supported.isEmpty {
                        SectionLabel("Supported now", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.supported) { row in
                                PickerDeviceRow(row: row, scale: scale)
                            }
                        }
                    }

                    if !sections.unsupported.isEmpty {
                        SectionLabel("Looks like a PEV, unsupported for launch", scale: scale)
                            .padding(.top, 8 * scale)
                        VStack(spacing: 12 * scale) {
                            ForEach(sections.unsupported) { row in
                                PickerDeviceRow(row: row, scale: scale)
                            }
                        }
                    }

                    if let manualRow = sections.manual {
                        ManualPickerRow(row: manualRow, scale: scale)
                            .padding(.top, 32 * scale)
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
    let text: String
    let scale: CGFloat

    var body: some View {
        HStack {
            Text(text)
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
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            let line = max(2, side * 0.08)

            ZStack {
                switch row.title {
                case "Aero-126V":
                    EucGlyph(color: row.glyphColor, lineWidth: line)
                case "Little FOCer BT":
                    OnewheelGlyph(color: row.glyphColor, accent: MockupColors.purple, lineWidth: line)
                case "NINEBOT-7A31":
                    ScooterGlyph(color: row.glyphColor, lineWidth: line)
                case "HX Hoverboard":
                    HoverboardGlyph(color: row.glyphColor, lineWidth: line)
                default:
                    Circle()
                        .fill(row.glyphBackground)
                    Image(systemName: row.symbolName)
                        .font(.system(size: side * 0.42, weight: .bold))
                        .foregroundStyle(row.glyphColor)
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
    }
}

private struct EucGlyph: View {
    let color: Color
    let lineWidth: CGFloat

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                Circle()
                    .fill(MockupColors.iconFill)
                Circle()
                    .stroke(color, lineWidth: lineWidth)
                Circle()
                    .fill(MockupColors.cardFill)
                    .frame(width: side * 0.42, height: side * 0.42)
                ForEach(0..<8, id: \.self) { index in
                    Circle()
                        .fill(color)
                        .frame(width: side * 0.085, height: side * 0.085)
                        .offset(y: -side * 0.16)
                        .rotationEffect(.degrees(Double(index) * 45))
                }
                Circle()
                    .fill(color)
                    .frame(width: side * 0.10, height: side * 0.10)
            }
        }
    }
}

private struct OnewheelGlyph: View {
    let color: Color
    let accent: Color
    let lineWidth: CGFloat

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                Capsule()
                    .stroke(accent, lineWidth: lineWidth * 0.8)
                    .frame(width: side * 0.92, height: side * 0.34)
                Circle()
                    .fill(MockupColors.iconFill)
                    .frame(width: side * 0.46, height: side * 0.46)
                Circle()
                    .stroke(color, lineWidth: lineWidth)
                    .frame(width: side * 0.46, height: side * 0.46)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
    }
}

private struct ScooterGlyph: View {
    let color: Color
    let lineWidth: CGFloat

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                Circle()
                    .stroke(color, lineWidth: lineWidth * 0.85)
                    .frame(width: side * 0.23, height: side * 0.23)
                    .offset(x: -side * 0.30, y: side * 0.22)
                Circle()
                    .stroke(color, lineWidth: lineWidth * 0.85)
                    .frame(width: side * 0.23, height: side * 0.23)
                    .offset(x: side * 0.32, y: side * 0.22)
                Path { path in
                    path.move(to: CGPoint(x: side * 0.20, y: side * 0.29))
                    path.addLine(to: CGPoint(x: side * 0.48, y: side * 0.74))
                    path.addLine(to: CGPoint(x: side * 0.75, y: side * 0.74))
                    path.move(to: CGPoint(x: side * 0.48, y: side * 0.74))
                    path.addLine(to: CGPoint(x: side * 0.26, y: side * 0.74))
                    path.move(to: CGPoint(x: side * 0.20, y: side * 0.29))
                    path.addLine(to: CGPoint(x: side * 0.34, y: side * 0.29))
                }
                .stroke(color, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round, lineJoin: .round))
                .frame(width: side, height: side)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
        }
    }
}

private struct HoverboardGlyph: View {
    let color: Color
    let lineWidth: CGFloat

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                Capsule()
                    .stroke(color, lineWidth: lineWidth)
                    .frame(width: side * 0.56, height: side * 0.26)
                    .offset(x: -side * 0.18)
                Capsule()
                    .stroke(color, lineWidth: lineWidth)
                    .frame(width: side * 0.56, height: side * 0.26)
                    .offset(x: side * 0.18)
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
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

    var titleColor: Color {
        isSupported ? MockupColors.primaryText : MockupColors.disabledText
    }

    var secondaryTextColor: Color {
        isSupported ? MockupColors.muted : MockupColors.disabledSecondaryText
    }
}

private enum MockupColors {
    static let pageBackground = Color(red: 0.027, green: 0.031, blue: 0.043)
    static let cardFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    static let cardStroke = Color(red: 0.165, green: 0.188, blue: 0.239)
    static let disabledFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    static let primaryText = Color(red: 0.969, green: 0.953, blue: 0.918)
    static let disabledText = Color(red: 0.455, green: 0.475, blue: 0.514)
    static let disabledSecondaryText = Color(red: 0.36, green: 0.38, blue: 0.42)
    static let muted = Color(red: 0.561, green: 0.596, blue: 0.659)
    static let yellow = Color(red: 1.0, green: 0.827, blue: 0.302)
    static let teal = Color(red: 0.180, green: 0.384, blue: 0.459)
    static let brown = Color(red: 0.443, green: 0.259, blue: 0.141)
    static let purple = Color(red: 0.635, green: 0.459, blue: 0.918)
    static let iconFill = Color(red: 0.043, green: 0.051, blue: 0.071)
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
