import CutoutMobile
import CutoutMobileFFI
import SwiftUI

enum LightingControlPage: String, CaseIterable {
    case color = "Color", effects = "Effects", music = "Music"
}

/// Wire-ID names transcribed from the MELK OA21 reference catalog (not OC21 visual verification).
/// https://gist.github.com/clienthax/5b3cc5fa68f7c4c943f2252eaa21d804
/// Do not merge a different model's numbering into gaps in this catalog.
enum LightingPatternCatalog {
    struct Group: Equatable {
        let name: String
        let ids: [Int]
    }

    static let groups: [Group] = [
        Group(name: "Basic", ids: [1, 2, 212] + Array(193...211) + Array(77...88) + Array(181...192)),
        Group(name: "Curtain", ids: Array(57...76)),
        Group(name: "Trans", ids: Array(3...22)),
        Group(name: "Water", ids: Array(39...56)),
        Group(name: "Flow", ids: Array(143...166)),
        Group(name: "Tail", ids: Array(23...38)),
        Group(name: "Run", ids: Array(stride(from: 89, through: 141, by: 2)) + Array(stride(from: 167, through: 179, by: 2))),
        Group(name: "Run Back", ids: Array(stride(from: 90, through: 142, by: 2)) + Array(stride(from: 168, through: 180, by: 2))),
        Group(name: "Unmapped", ids: [0] + Array(213...220)),
    ]

    static func name(for id: Int) -> String {
        guard id > 0, id < names.count else { return "Unmapped effect \(id)" }
        return names[id]
    }

    private static let names: [String] = [
        "", // 0
        "Magic Forward", // 1
        "Magic Back", // 2
        "7-Color Trans", // 3
        "7-Color Trans Back", // 4
        "R-G-B Trans", // 5
        "R-G-B Trans Back", // 6
        "Y-C-P Trans", // 7
        "Y-C-P Trans Back", // 8
        "6-Color to Red", // 9
        "6-Color to Red Back", // 10
        "6-Color to Green", // 11
        "6-Color to Green Back", // 12
        "6-Color to Blue", // 13
        "6-Color to Blue Back", // 14
        "6-Color to Cyan", // 15
        "6-Color to Cyan Back", // 16
        "6-Color to Yellow", // 17
        "6-Color to Yellow Back", // 18
        "6-Color to Purple", // 19
        "6-Color to Purple Back", // 20
        "6-Color to White", // 21
        "6-Color to White Back", // 22
        "7-Color Tail", // 23
        "7-Color Tail Back", // 24
        "Red Tail", // 25
        "Red Tail Back", // 26
        "Green Tail", // 27
        "Green Tail Back", // 28
        "Blue Tail", // 29
        "Blue Tail Back", // 30
        "Yellow Tail", // 31
        "Yellow Tail Back", // 32
        "Cyan Tail", // 33
        "Cyan Tail Back", // 34
        "Purple Tail", // 35
        "Purple Tail Back", // 36
        "White Tail", // 37
        "White Tail Back", // 38
        "7-Color Water", // 39
        "7-Color Water Back", // 40
        "R-G-B Water", // 41
        "R-G-B Water Back", // 42
        "Y-C-P Water", // 43
        "Y-C-P Water Back", // 44
        "R-G Water", // 45
        "R-G Water Back", // 46
        "G-B Water", // 47
        "G-B Water Back", // 48
        "Y-B Water", // 49
        "Y-B Water Back", // 50
        "Y-C Water", // 51
        "Y-C Water Back", // 52
        "C-P Water", // 53
        "C-P Water Back", // 54
        "White Water", // 55
        "White Water Back", // 56
        "7-Color Close", // 57
        "7-Color Open", // 58
        "R-G-B Close", // 59
        "R-G-B Open", // 60
        "Y-C-P Close", // 61
        "Y-C-P Open", // 62
        "Red Close", // 63
        "Red Open", // 64
        "Green Close", // 65
        "Green Open", // 66
        "Blue Close", // 67
        "Blue Open", // 68
        "Yellow Close", // 69
        "Yellow Open", // 70
        "Cyan Close", // 71
        "Cyan Open", // 72
        "Purple Close", // 73
        "Purple Open", // 74
        "White Close", // 75
        "White Open", // 76
        "7-Color Race", // 77
        "7-Color Race Back", // 78
        "R-G-B Race", // 79
        "R-G-B Race Back", // 80
        "Y-C-P Race", // 81
        "Y-C-P Race Back", // 82
        "7-Color Wave", // 83
        "7-Color Wave Back", // 84
        "R-G-B Wave", // 85
        "R-G-B Wave Back", // 86
        "Y-C-P Wave", // 87
        "Y-C-P Wave Back", // 88
        "Red Running", // 89
        "Red Run Back", // 90
        "Green Running", // 91
        "Green Run Back", // 92
        "Blue Running", // 93
        "Blue Run Back", // 94
        "Yellow Running", // 95
        "Yellow Run Back", // 96
        "Cyan Running", // 97
        "Cyan Run Back", // 98
        "Purple Running", // 99
        "Purple Run Back", // 100
        "White Running", // 101
        "White Run Back", // 102
        "7-Color Running", // 103
        "7-Color Run Back", // 104
        "R-G-B Running", // 105
        "R-G-B Run Back", // 106
        "Y-C-P Running", // 107
        "Y-C-P Run Back", // 108
        "B-P-C-Y Running", // 109
        "B-P-C-Y Run Back", // 110
        "B-G-C-Y Running", // 111
        "B-G-C-Y Run Back", // 112
        "Red-Dot in White Running", // 113
        "Red-Dot in White Run Back", // 114
        "Green-Dot in Red Running", // 115
        "Green-Dot in Red Run Back", // 116
        "Blue-Dot in Green Running", // 117
        "Blue-Dot in Green Run Back", // 118
        "Yellow-Dot in Blue Running", // 119
        "Yellow-Dot in Blue Run Back", // 120
        "Cyan-Dot in Yellow Running", // 121
        "Cyan-Dot in Yellow Run Back", // 122
        "Purple-Dot in Cyan Running", // 123
        "Purple-Dot in Cyan Run Back", // 124
        "White-Dot in Purple Running", // 125
        "White-Dot in Purple Run Back", // 126
        "White-Dot in Red Running", // 127
        "White-Dot in Red Run Back", // 128
        "7-Color in Red Running", // 129
        "7-Color in Red Run Back", // 130
        "7-Color in Green Running", // 131
        "7-Color in Green Run Back", // 132
        "7-Color in Blue Running", // 133
        "7-Color in Blue Run Back", // 134
        "7-Color in Yellow Running", // 135
        "7-Color in Yellow Run Back", // 136
        "7-Color in Cyan Running", // 137
        "7-Color in Cyan Run Back", // 138
        "7-Color in Purple Running", // 139
        "7-Color in Purple Run Back", // 140
        "7-Color in White Running", // 141
        "7-Color in White Run Back", // 142
        "W-R-W Flow", // 143
        "W-R-W Flow Back", // 144
        "W-G-W Flow", // 145
        "W-G-W Flow Back", // 146
        "W-B-W Flow", // 147
        "W-B-W Flow Back", // 148
        "W-Y-W Flow", // 149
        "W-Y-W Flow Back", // 150
        "W-C-W Flow", // 151
        "W-C-W Flow Back", // 152
        "W-P-W Flow", // 153
        "W-P-W Flow Back", // 154
        "R-W-R Flow", // 155
        "R-W-R Flow Back", // 156
        "G-W-G Flow", // 157
        "G-W-G Flow Back", // 158
        "B-W-B Flow", // 159
        "B-W-B Flow Back", // 160
        "Y-W-Y Flow", // 161
        "Y-W-Y Flow Back", // 162
        "C-W-C Flow", // 163
        "C-W-C Flow Back", // 164
        "P-W-P Flow", // 165
        "P-W-P Flow Back", // 166
        "Green-Dot in Blue Running", // 167
        "Green-Dot in Blue Run Back", // 168
        "Green-Dot in Red Running", // 169
        "Green-Dot in Red Run Back", // 170
        "Red-Dot in Blue Running", // 171
        "Red-Dot in Blue Run Back", // 172
        "Cyan-Dot in Yellow Running", // 173
        "Cyan-Dot in Yellow Run Back", // 174
        "Yellow-Dot in Purple Running", // 175
        "Yellow-Dot in Purple Run Back", // 176
        "White-Dot in Yellow Running", // 177
        "White-Dot in Yellow Run Back", // 178
        "Yellow-Dot in White Running", // 179
        "Yellow-Dot in White Run Back", // 180
        "7-Color Flush", // 181
        "7-Color Flush Back", // 182
        "R-G-B Flush", // 183
        "R-G-B Flush Back", // 184
        "Y-C-P Flush", // 185
        "Y-C-P Flush Back", // 186
        "7-Color Flush Close", // 187
        "7-Color Flush Open", // 188
        "R-G-B Flush Close", // 189
        "R-G-B Flush Open", // 190
        "Y-C-P Flush Close", // 191
        "Y-C-P Flush Open", // 192
        "7-Color Jump", // 193
        "R-G-B Jump", // 194
        "Y-C-P Jump", // 195
        "7-Color Strobe", // 196
        "R-G-B Strobe", // 197
        "Y-C-P Strobe", // 198
        "7-Color Gradual", // 199
        "R-Y Gradual", // 200
        "R-P Gradual", // 201
        "G-C Gradual", // 202
        "G-Y Gradual", // 203
        "B-P Gradual", // 204
        "Red Marquee", // 205
        "Green Marquee", // 206
        "Blue Marquee", // 207
        "Yellow Marquee", // 208
        "Cyan Marquee", // 209
        "Purple Marquee", // 210
        "White Marquee", // 211
        "7-Color Energy", // 212
    ]
}

/// Controller-local playback controls. Selection is a draft until the user presses Play.
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

    private let favorites: [(id: Int, symbol: String)] = [
        (1, "water.waves"), (16, "arrow.right"), (22, "rainbow"), (75, "sparkles"),
    ]
    private let musicNames = ["Energetic", "Rhythm", "Spectrum", "Rolling", "Energetic 2", "Rhythm 2", "Spectrum 2", "Rolling 2"]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if page == .effects {
                Label("Patterns", systemImage: "sparkles").font(.headline)
                Picker("Pattern group", selection: $group) {
                    ForEach(LightingPatternCatalog.groups, id: \.name) { group in
                        Text(group.name).tag(group.name)
                    }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("lighting.pattern-group")
                Text("Quick picks")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                effectGrid(ids: favorites.map(\.id), symbolForID: { id in
                    favorites.first(where: { $0.id == id })?.symbol ?? "sparkles"
                })
                Text("\(group) patterns")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
                effectGrid(ids: patternIDs, symbolForID: { _ in "sparkles" })
                Picker("Pattern", selection: $pattern) {
                    ForEach(patternIDs, id: \.self) { id in
                        Text("\(id) · \(LightingPatternCatalog.name(for: id))").tag(id)
                    }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("lighting.pattern")
                HStack {
                    Label("Speed", systemImage: "speedometer")
                    Spacer()
                    Text("\(Int(((255 - speed) * 100 / 255).rounded()))%").monospacedDigit().foregroundStyle(.secondary)
                }
                Slider(value: Binding(
                    get: { 255 - speed },
                    set: { speed = 255 - $0 }
                ), in: 0...255, step: 1) {
                    Text("Effect speed")
                } onEditingChanged: { editing in
                    if !editing, case let .effect(activePattern, _) = model.requestedPlayback,
                       Int(activePattern) == pattern {
                        model.setEffectSpeed(UInt8(speed))
                    }
                }
                .tint(.purple)
                .accessibilityIdentifier("lighting.effect-speed")
                .accessibilityValue("\(Int(((255 - speed) * 100 / 255).rounded())) percent")
                HStack {
                    Text("Slower")
                    Spacer()
                    Text("Faster")
                }
                .font(.caption).foregroundStyle(PevColors.muted)
                Text("Reference names may vary by firmware.")
                    .font(.caption)
                    .foregroundStyle(PevColors.muted)
                Text("Unmapped entries retain their IDs and are unavailable until this controller is captured.")
                    .font(.caption)
                    .foregroundStyle(PevColors.muted)
            } else {
                Label("Music", systemImage: "waveform").font(.headline)
                Text("Uses the microphone on your light controller. Your phone does not record audio.")
                    .font(.subheadline).foregroundStyle(PevColors.muted)
                Picker("Music effect", selection: $musicEffect) {
                    ForEach(musicNames.indices, id: \.self) { index in
                        Text(musicNames[index]).tag(index)
                    }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("lighting.music-effect")
                HStack {
                    Label("Sensitivity", systemImage: "mic")
                    Spacer()
                    Text("\(Int(sensitivity))%").monospacedDigit().foregroundStyle(.secondary)
                }
                Slider(value: $sensitivity, in: 0...100, step: 1) {
                    Text("Microphone sensitivity")
                } onEditingChanged: { editing in
                    if !editing, case .music = model.requestedPlayback { play() }
                }
                .tint(.pink)
                .accessibilityIdentifier("lighting.music-sensitivity")
            }
            Button(action: play) {
                Label(page == .effects ? "Play pattern" : "Start music mode", systemImage: "play.fill")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .tint(page == .effects ? .purple : .pink)
            .accessibilityIdentifier("lighting.play-mode")
            if page == .music {
                Button("Stop music mode") { model.stopMusic() }
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

    @ViewBuilder
    private func effectGrid(ids: [Int], symbolForID: @escaping (Int) -> String) -> some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 125))], spacing: 10) {
            ForEach(ids, id: \.self) { id in
                Button {
                    pattern = id
                    selectGroup(for: id)
                    play()
                } label: {
                    VStack(spacing: 10) {
                        Image(systemName: symbolForID(id))
                            .font(.title2)
                            .foregroundStyle(
                                LinearGradient(
                                    colors: [PevColors.cyan, .purple, .pink],
                                    startPoint: .leading,
                                    endPoint: .trailing
                                )
                            )
                        Text(LightingPatternCatalog.name(for: id))
                            .font(.subheadline)
                            .multilineTextAlignment(.center)
                            .lineLimit(2)
                        Text("ID \(id)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, minHeight: 86)
                    .background(
                        .purple.opacity(pattern == id ? 0.22 : 0.08),
                        in: RoundedRectangle(cornerRadius: 14)
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 14)
                            .stroke(
                                pattern == id ? Color.purple : PevColors.cardStroke,
                                lineWidth: 1
                            )
                    )
                }
                .buttonStyle(.plain)
                .disabled(!canSendPattern(id))
                .accessibilityValue(canSendPattern(id) ? "available" : "unmapped, unavailable")
                .accessibilityIdentifier("lighting.effect.\(id)")
            }
        }
    }

    private func selectGroup(for id: Int) {
        if let selected = LightingPatternCatalog.groups.first(where: { $0.ids.contains(id) }) {
            group = selected.name
        }
    }

    private func canSendPattern(_ id: Int) -> Bool {
        (1...212).contains(id)
    }

    private func play() {
        if page == .effects {
            guard canSendPattern(pattern) else { return }
            model.setPlayback(.effect(pattern: UInt8(pattern), speed: UInt8(speed)))
        } else {
            model.setPlayback(.music(effect: UInt8(musicEffect), sensitivity: UInt8(sensitivity)))
        }
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
                Text("Manual local-time timers run on the controller. Sunrise/sunset automation and timer readback are not available for this profile.")
                    .font(.footnote)
                    .foregroundStyle(PevColors.muted)

                timerRow(
                    title: "Turn on",
                    powerOn: true,
                    time: $onTime,
                    days: $onDays,
                    enabled: $onEnabled
                )
                Divider()
                timerRow(
                    title: "Turn off",
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
            Label("Schedule", systemImage: "clock").font(.headline)
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
                Toggle("Enabled", isOn: enabled)
                    .labelsHidden()
                    .tint(PevColors.cyan)
                    .accessibilityLabel("\(title) timer enabled")
            }
            DatePicker(
                "\(title) time",
                selection: time,
                displayedComponents: .hourAndMinute
            )
            .datePickerStyle(.compact)
            .accessibilityIdentifier("lighting.schedule.\(powerOn ? "on" : "off").time")

            WeekdayMaskPicker(days: days, identifier: powerOn ? "on" : "off")
            Button("Save \(title.lowercased()) timer") {
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
        feedback = "\(powerOn ? "Turn-on" : "Turn-off") timer requested"
    }

    private static func defaultTime(hour: Int) -> Date {
        Calendar.current.date(from: DateComponents(hour: hour, minute: 0)) ?? Date()
    }
}

private struct WeekdayMaskPicker: View {
    @Binding var days: UInt8
    let identifier: String
    private let labels = ["M", "T", "W", "T", "F", "S", "S"]

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
                .accessibilityLabel("weekday \(index + 1)")
                .accessibilityValue(days & bit == 0 ? "off" : "on")
                .accessibilityIdentifier("lighting.schedule.\(identifier).day.\(index + 1)")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Repeat days")
    }
}
