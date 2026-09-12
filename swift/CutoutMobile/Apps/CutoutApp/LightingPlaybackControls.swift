import Foundation
import CutoutMobile
import CutoutMobileFFI
import SwiftUI

enum LightingControlPage: String, CaseIterable {
    case color
    case effects
    case music
    case schedule

    var title: String {
        localizedAppText("lighting.page.\(rawValue)")
    }
}

/// Rust-owned MELK effect catalog adapted for SwiftUI layout and localization.
enum LightingPatternCatalog {
    struct Group: Equatable {
        let name: String
        let ids: [Int]
    }

    private static let patterns: [MobileMelkLightingPatternDto] = mobileMelkLightingPatternCatalog()
    private static let patternsByID: [Int: MobileMelkLightingPatternDto] = Dictionary(
        uniqueKeysWithValues: patterns.map { (Int($0.id), $0) }
    )

    /// Group membership is protocol metadata owned by Rust; this view only adapts it for layout.
    static let groups: [Group] = mobileMelkLightingEffectGroups().map { group in
        Group(name: group.name, ids: group.effectIds.map(Int.init))
    }

    static func name(for id: Int) -> String {
        guard let pattern = patternsByID[id] else {
            return "Unmapped effect \(id)"
        }
        return pattern.name
    }

    static func groupName(for name: String) -> String {
        let key = name
            .lowercased()
            .replacingOccurrences(of: " ", with: "_")
        return localizedAppText("lighting.effect.group.\(key)")
    }
}

private enum LightingEffectPreviewModel {
    static func points(for id: Int, in size: CGSize) -> [CGPoint] {
        let width = max(size.width, 1)
        let height = max(size.height, 1)
        let horizontalPadding = min(14, width / 5)
        let verticalPadding = min(10, height / 5)
        let usableWidth = max(width - (horizontalPadding * 2), 1)
        let usableHeight = max(height - (verticalPadding * 2), 1)
        let count = 30

        switch LightingPatternCatalog.groups.first(where: { $0.ids.contains(id) })?.name {
        case "Curtain":
            return (0..<count).map { index in
                let column = Double(index % 5) / 4
                let progress = Double(index / 5) / 5
                let x = horizontalPadding + (usableWidth * column)
                let y = verticalPadding + (usableHeight * (0.18 + abs(sin(progress * .pi)) * 0.72))
                return CGPoint(x: x, y: y)
            }
        case "Run", "Run Back":
            return (0..<count).map { index in
                let progress = Double(index) / Double(count - 1)
                let x = horizontalPadding + (usableWidth * progress)
                let wave = sin((progress * 3 + Double(id % 5) * 0.4) * .pi)
                let y = verticalPadding + (usableHeight * (0.5 + wave * 0.3))
                return CGPoint(x: x, y: y)
            }
        case "Water", "Flow":
            return spiralPoints(
                count: count,
                center: CGPoint(x: width / 2, y: height / 2),
                radius: min(usableWidth, usableHeight) * 0.42,
                turns: id.isMultiple(of: 2) ? 2.5 : 3.5
            )
        case "Tail":
            return (0..<count).map { index in
                let progress = Double(index) / Double(count - 1)
                let x = horizontalPadding + (usableWidth * progress)
                let y = verticalPadding + (usableHeight * (0.8 - progress * 0.55))
                return CGPoint(x: x, y: y)
            }
        default:
            return wavePoints(
                count: count,
                width: width,
                height: height,
                horizontalPadding: horizontalPadding,
                verticalPadding: verticalPadding,
                reverse: id.isMultiple(of: 2)
            )
        }
    }

    static func colors(for id: Int) -> [Color] {
        let baseHue = Double((abs(id) * 37) % 360) / 360
        return (0..<4).map { index in
            let hue = (baseHue + Double(index) * 0.11).truncatingRemainder(dividingBy: 1)
            return Color(hue: hue, saturation: 0.9, brightness: 1)
        }
    }

    private static func wavePoints(
        count: Int,
        width: CGFloat,
        height: CGFloat,
        horizontalPadding: CGFloat,
        verticalPadding: CGFloat,
        reverse: Bool
    ) -> [CGPoint] {
        (0..<count).map { index in
            let progress = Double(index) / Double(count - 1)
            let xProgress = reverse ? 1 - progress : progress
            let y = verticalPadding + (height - (verticalPadding * 2)) * (0.5 + 0.3 * sin(progress * 3 * .pi))
            return CGPoint(
                x: horizontalPadding + (width - (horizontalPadding * 2)) * xProgress,
                y: y
            )
        }
    }

    private static func spiralPoints(
        count: Int,
        center: CGPoint,
        radius: CGFloat,
        turns: Double
    ) -> [CGPoint] {
        (0..<count).map { index in
            let progress = Double(index) / Double(count - 1)
            let angle = progress * turns * 2 * .pi
            let currentRadius = radius * (0.12 + progress * 0.88)
            return CGPoint(
                x: center.x + cos(angle) * currentRadius,
                y: center.y + sin(angle) * currentRadius * 0.72
            )
        }
    }
}

private struct LightingEffectPoint: Identifiable {
    let id: Int
    let point: CGPoint
}

private struct LightingEffectPreview: View {
    let patternID: Int

    var body: some View {
        GeometryReader { proxy in
            let points = LightingEffectPreviewModel.points(for: patternID, in: proxy.size)
            let colors = LightingEffectPreviewModel.colors(for: patternID)
            ZStack {
                ForEach(points.enumerated().map { index, point in
                    LightingEffectPoint(id: index, point: point)
                }) { dot in
                    Circle()
                        .fill(colors[dot.id % colors.count])
                        .frame(width: 6, height: 6)
                        .shadow(color: colors[dot.id % colors.count], radius: 4)
                        .position(dot.point)
                }
            }
        }
        .frame(height: 72)
        .padding(.horizontal, 6)
        .background(Color.black.opacity(0.18), in: RoundedRectangle(cornerRadius: 10))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .accessibilityHidden(true)
    }
}

private struct LightingEffectGrid: View {
    let ids: [Int]
    let selectedPattern: Int
    let onSelect: (Int) -> Void

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 125))], spacing: 10) {
            ForEach(ids, id: \.self) { id in
                Button {
                    onSelect(id)
                } label: {
                    VStack(spacing: 8) {
                        LightingEffectPreview(patternID: id)
                        Text(LightingPatternCatalog.name(for: id))
                            .font(.subheadline)
                            .multilineTextAlignment(.center)
                            .lineLimit(2)
                        Text(localizedAppText("lighting.effect.id", Int64(id)))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, minHeight: 128)
                    .background(
                        .purple.opacity(selectedPattern == id ? 0.22 : 0.08),
                        in: RoundedRectangle(cornerRadius: 14)
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 14)
                            .stroke(
                                selectedPattern == id ? Color.purple : PevColors.cardStroke,
                                lineWidth: 1
                            )
                    )
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("lighting.effect.\(id)")
            }
        }
    }
}

/// Controller-local playback controls. Selecting a mode sends it immediately.
struct LightingPlaybackControls: View {
    let model: LightingRouteModel
    let page: LightingControlPage
    @State private var pattern = 1
    @State private var group = "Basic"

    private var patternIDs: [Int] {
        LightingPatternCatalog.groups.first(where: { $0.name == group })?.ids ?? []
    }

    @State private var speed = 128.0
    @State private var musicEffect = 0
    @State private var sensitivity = 50.0

    private let favorites = [1, 16, 22, 75]
    private let musicNames = [
        "lighting.music.effect.flow_flash",
        "lighting.music.effect.flash",
        "lighting.music.effect.rainbow",
        "lighting.music.effect.snake",
        "lighting.music.effect.rainbow_2",
        "lighting.music.effect.pulse",
        "lighting.music.effect.flow",
        "lighting.music.effect.pulse_2",
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if page == .effects {
                Label(localizedAppText("lighting.patterns"), systemImage: "sparkles").font(.headline)
                Picker(localizedAppText("lighting.pattern_group"), selection: $group) {
                    ForEach(LightingPatternCatalog.groups, id: \.name) { group in
                        Text(LightingPatternCatalog.groupName(for: group.name)).tag(group.name)
                    }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("lighting.pattern-group")
                Text(localizedAppText("lighting.quick_picks"))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                LightingEffectGrid(
                    ids: favorites,
                    selectedPattern: pattern,
                    onSelect: selectPattern
                )
                Text(localizedAppText("lighting.group_patterns", LightingPatternCatalog.groupName(for: group)))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                LightingEffectGrid(
                    ids: patternIDs,
                    selectedPattern: pattern,
                    onSelect: selectPattern
                )
                Picker(localizedAppText("lighting.pattern"), selection: Binding(
                    get: { pattern },
                    set: { selectPattern($0) }
                )) {
                    ForEach(patternIDs, id: \.self) { id in
                        Text(localizedAppText("lighting.effect.option", Int64(id), LightingPatternCatalog.name(for: id))).tag(id)
                    }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("lighting.pattern")
                HStack {
                    Label(localizedAppText("lighting.speed"), systemImage: "speedometer")
                    Spacer()
                    Text(localizedAppText("lighting.percent", Int64(((255 - speed) * 100 / 255).rounded())))
                        .monospacedDigit().foregroundStyle(.secondary)
                }
                Slider(value: Binding(
                    get: { 255 - speed },
                    set: { speed = 255 - $0 }
                ), in: 0...255, step: 1) {
                    Text(localizedAppText("lighting.effect.speed"))
                } onEditingChanged: { editing in
                    if !editing, case let .effect(activePattern, _) = model.requestedPlayback,
                       Int(activePattern) == pattern {
                        if !model.setEffectSpeed(UInt8(speed)),
                           case let .effect(_, currentSpeed) = model.requestedPlayback {
                            speed = Double(currentSpeed)
                        }
                    }
                }
                .tint(.purple)
                .accessibilityIdentifier("lighting.effect-speed")
                .accessibilityValue(localizedAppText("lighting.percent_accessibility", Int64(((255 - speed) * 100 / 255).rounded())))
                HStack {
                    Text(localizedAppText("lighting.slower"))
                    Spacer()
                    Text(localizedAppText("lighting.faster"))
                }
                .font(.caption).foregroundStyle(PevColors.muted)
            } else {
                Label(localizedAppText("lighting.page.music"), systemImage: "waveform").font(.headline)
                Picker(localizedAppText("lighting.music.effect"), selection: Binding(
                    get: { musicEffect },
                    set: {
                        musicEffect = $0
                        applyMusic()
                    }
                )) {
                    ForEach(musicNames.indices, id: \.self) { index in
                        Text(localizedAppText(musicNames[index])).tag(index)
                    }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("lighting.music-effect")
                HStack {
                    Label(localizedAppText("lighting.sensitivity"), systemImage: "mic")
                    Spacer()
                    Text(localizedAppText("lighting.percent", Int64(sensitivity)))
                        .monospacedDigit().foregroundStyle(.secondary)
                }
                Slider(value: $sensitivity, in: 0...100, step: 1) {
                    Text(localizedAppText("lighting.microphone_sensitivity"))
                } onEditingChanged: { editing in
                    if !editing, case .music = model.requestedPlayback { applyMusic() }
                }
                .tint(.pink)
                .accessibilityIdentifier("lighting.music-sensitivity")
            }
            if page == .music {
                Button(localizedAppText("lighting.stop_music")) { model.stopMusic() }
                    .buttonStyle(.bordered)
                    .frame(maxWidth: .infinity)
                    .accessibilityIdentifier("lighting.stop-music")
            }
        }
        .disabled(!model.isReady)
        .padding(16)
        .background(PevDashboardCardBackground(cornerRadius: 20))
        .onChange(of: group) {
            if !patternIDs.contains(pattern), let first = patternIDs.first { pattern = first }
        }
        .onChange(of: model.requestedPlayback, initial: true) { _, playback in
            switch playback {
            case let .effect(selected, selectedSpeed):
                pattern = Int(selected)
                selectGroup(for: pattern)
                speed = Double(selectedSpeed)
            case let .music(selected, gain):
                musicEffect = Int(selected)
                sensitivity = Double(gain)
            case .solid:
                break
            }
        }
    }

    func selectPattern(_ id: Int) {
        pattern = id
        selectGroup(for: id)
        model.setPlayback(.effect(pattern: UInt8(id), speed: UInt8(speed)))
    }

    private func selectGroup(for id: Int) {
        if let selected = LightingPatternCatalog.groups.first(where: { $0.ids.contains(id) }) {
            group = selected.name
        }
    }

    private func applyMusic() {
        model.setPlayback(.music(effect: UInt8(musicEffect), sensitivity: UInt8(sensitivity)))
    }
}

/// The controller owns two timer slots: one for turning on and one for turning off.
struct LightingScheduleControls: View {
    let model: LightingRouteModel
    @State private var isExpanded = false
    @State private var onTime = Self.defaultTime(hour: 18)
    @State private var offTime = Self.defaultTime(hour: 23)
    @State private var onDays: UInt8 = 0x7f
    @State private var offDays: UInt8 = 0x7f
    @State private var onEnabled = false
    @State private var offEnabled = false
    @State private var feedback: String?

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            VStack(alignment: .leading, spacing: 16) {
                timerRow(
                    title: localizedAppText("lighting.schedule.turn_on"),
                    powerOn: true,
                    time: $onTime,
                    days: $onDays,
                    enabled: $onEnabled
                )
                Divider()
                timerRow(
                    title: localizedAppText("lighting.schedule.turn_off"),
                    powerOn: false,
                    time: $offTime,
                    days: $offDays,
                    enabled: $offEnabled
                )

                if let feedback {
                    Label(feedback, systemImage: "checkmark.circle")
                        .font(.footnote)
                        .foregroundStyle(PevColors.green)
                }
            }
            .padding(.top, 12)
        } label: {
            Label(localizedAppText("lighting.page.schedule"), systemImage: "clock").font(.headline)
        }
        .padding(16)
        .background(PevDashboardCardBackground(cornerRadius: 20))
        .accessibilityIdentifier("lighting.schedule")
    }

    @ViewBuilder
    private func timerRow(
        title: String,
        powerOn: Bool,
        time: Binding<Date>,
        days: Binding<UInt8>,
        enabled: Binding<Bool>
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Label(title, systemImage: powerOn ? "lightbulb.fill" : "lightbulb.slash")
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Toggle(localizedAppText("lighting.schedule.enabled"), isOn: enabled)
                    .labelsHidden()
                    .tint(PevColors.cyan)
                    .accessibilityLabel(localizedAppText("lighting.schedule.enabled_for", title))
            }
            DatePicker(
                localizedAppText("lighting.schedule.time", title),
                selection: time,
                displayedComponents: .hourAndMinute
            )
            .datePickerStyle(.compact)
            .accessibilityIdentifier("lighting.schedule.\(powerOn ? "on" : "off").time")

            WeekdayMaskPicker(days: days, identifier: powerOn ? "on" : "off")
            Button(localizedAppText("lighting.schedule.save", title.lowercased())) {
                save(powerOn: powerOn, time: time.wrappedValue, days: days.wrappedValue, enabled: enabled.wrappedValue)
            }
            .buttonStyle(.bordered)
            .disabled(!model.isReady)
            .accessibilityIdentifier("lighting.schedule.\(powerOn ? "on" : "off").save")
        }
    }

    private func save(powerOn: Bool, time: Date, days: UInt8, enabled: Bool) {
        let components = Calendar.current.dateComponents([.hour, .minute], from: time)
        guard let hour = components.hour, let minute = components.minute else { return }
        let schedule = MobileMelkScheduleDto(
            powerOn: powerOn,
            hour: UInt8(hour),
            minute: UInt8(minute),
            days: days,
            enabled: enabled
        )
        guard model.setSchedule(schedule) else {
            feedback = nil
            return
        }
        feedback = localizedAppText(
            powerOn ? "lighting.schedule.requested_on" : "lighting.schedule.requested_off"
        )
    }

    private static func defaultTime(hour: Int) -> Date {
        Calendar.current.date(from: DateComponents(hour: hour, minute: 0)) ?? Date()
    }
}

private struct WeekdayMaskPicker: View {
    @Binding var days: UInt8
    let identifier: String
    private let labels = ["M", "T", "W", "T", "F", "S", "S"]

    private var weekdayNames: [String] {
        let symbols = Calendar.current.weekdaySymbols
        guard symbols.count == 7 else {
            // The protocol mask is Monday-first. Keep a semantic fallback if a platform
            // calendar cannot provide localized weekday symbols.
            return ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
        }
        return Array(symbols.dropFirst()) + [symbols[0]]
    }

    var body: some View {
        HStack(spacing: 6) {
            ForEach(0..<labels.count, id: \.self) { index in
                let bit = UInt8(1 << index)
                Button {
                    days ^= bit
                } label: {
                    Text(labels[index])
                        .font(.caption.weight(.semibold))
                        .frame(width: 30, height: 30)
                        .foregroundStyle(days & bit == 0 ? PevColors.muted : PevColors.primaryText)
                        .background(
                            (days & bit == 0 ? PevColors.cardStroke : PevColors.cyan).opacity(0.25),
                            in: Circle()
                        )
                }
                .buttonStyle(.plain)
                .accessibilityLabel(weekdayNames[index])
                .accessibilityValue(localizedAppText(days & bit == 0 ? "lighting.off" : "lighting.on"))
                .accessibilityIdentifier("lighting.schedule.\(identifier).day.\(index + 1)")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(localizedAppText("lighting.repeat_days"))
    }
}
