import CutoutMobile
import CutoutMobileFFI
import Foundation
import SwiftUI

struct LightingRouteView: View {
    let model: LightingRouteModel
    let rideModel: CutoutAppModel
    @State private var page: LightingControlPage = .color
    @State private var brightness = 100.0
    @State private var hue = 0.0
    @State private var saturation = 1.0
    @State private var showsPairing = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                LightingScreenHeader()
                LightingConnectionCard(model: model, rideModel: rideModel) {
                    showsPairing = true
                }
                LightingPagePicker(selection: $page)
                LightingControlSurface(
                    model: model,
                    page: page,
                    hue: $hue,
                    saturation: $saturation
                )
                LightingBrightnessControl(
                    brightness: $brightness,
                    isEnabled: model.isReady,
                    onCommit: commitBrightness
                )
                LightingPresetsCard(
                    model: model,
                    isEnabled: model.isReady,
                    hue: $hue,
                    saturation: $saturation
                )
                LightingScheduleControls(model: model)
                LightingErrorBanner(error: model.controlError)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
        }
        .background(PevColors.pageBackground.ignoresSafeArea())
        .scrollDismissesKeyboard(.interactively)
        .onChange(of: model.requestedBrightness) { _, value in
            brightness = Double(value)
        }
        .task {
            startLighting()
        }
        .sheet(isPresented: $showsPairing) {
            LightingPairingSheet(model: model, rideModel: rideModel)
        }
        .accessibilityIdentifier("dashboard.screen.lighting")
    }

    private func startLighting() {
        model.start()
        brightness = Double(model.requestedBrightness)
        updateColorSelection()
    }

    private func commitBrightness() {
        guard model.isReady else { return }
        model.setBrightness(UInt8(brightness.rounded()))
    }

    private func updateColorSelection() {
        let selection = lightingColorSelection(
            red: model.requestedRed,
            green: model.requestedGreen,
            blue: model.requestedBlue
        )
        hue = selection.hue
        saturation = selection.saturation
    }
}

private struct LightingScreenHeader: View {
    var body: some View {
        Text("Lighting")
            .font(.largeTitle.weight(.bold))
            .foregroundStyle(PevColors.primaryText)
            .accessibilityHeading(.h1)
    }
}

private struct LightingPagePicker: View {
    @Binding var selection: LightingControlPage

    var body: some View {
        Picker("Lighting controls", selection: $selection) {
            ForEach(LightingControlPage.allCases, id: \.self) { page in
                Text(page.rawValue).tag(page)
            }
        }
        .pickerStyle(.segmented)
        .accessibilityIdentifier("lighting.control-page")
    }
}

private struct LightingControlSurface: View {
    let model: LightingRouteModel
    let page: LightingControlPage
    @Binding var hue: Double
    @Binding var saturation: Double

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if page == .color {
                LightingColorControls(model: model, hue: $hue, saturation: $saturation)
            } else {
                LightingCard {
                    LightingPowerToggle(model: model)
                }
                LightingPlaybackControls(model: model, page: page)
            }
        }
    }
}

private struct LightingPowerToggle: View {
    let model: LightingRouteModel

    var body: some View {
        Toggle(
            "Power",
            isOn: Binding(
                get: { model.requestedPowerOn },
                set: model.setPower
            )
        )
        .font(.headline)
        .tint(PevColors.cyan)
        .disabled(!model.isReady)
        .accessibilityIdentifier("lighting.power")
    }
}

private struct LightingConnectionCard: View {
    let model: LightingRouteModel
    let rideModel: CutoutAppModel
    let onDetails: () -> Void

    var body: some View {
        LightingCard {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "lightbulb.led.fill")
                    .font(.title2)
                    .foregroundStyle(PevColors.cyan)
                    .frame(width: 34, height: 34)
                    .background(PevColors.cyan.opacity(0.14), in: Circle())
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.accessoryAlias ?? model.peripheralName ?? "MELK-OC21 6A")
                        .font(.headline)
                    Text(connectionSummary)
                        .font(.subheadline)
                        .foregroundStyle(connectionStatusColor)
                }
                Spacer(minLength: 8)
                LightingConnectionPill(model: model, color: connectionStatusColor)
                Button(action: onDetails) {
                    Image(systemName: "info.circle")
                        .font(.title3)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Lighting accessory details")
                .accessibilityIdentifier("lighting.accessory-details")
            }
            Label(
                "Ride stays \(rideModel.connectionStatusText); lighting uses an independent Bluetooth connection.",
                systemImage: "figure.roll"
            )
            .font(.footnote)
            .foregroundStyle(PevColors.muted)
        }
    }

    private var connectionSummary: String {
        switch model.connectionState {
        case .ready: "Connected"
        case .scanning: "Scanning for nearby accessories…"
        case .connecting, .discovering: "Connecting…"
        case let .retrying(attempt, delayMilliseconds):
            "Retrying (\(attempt)) in \(max(1, Int((delayMilliseconds + 999) / 1000)))s…"
        case .disconnected: "Not connected"
        case .failed: "Connection failed"
        case .idle: "Ready to scan"
        }
    }

    private var connectionStatusColor: Color {
        switch model.connectionState {
        case .ready: PevColors.green
        case .failed: PevColors.red
        default: PevColors.yellow
        }
    }
}

private struct LightingConnectionPill: View {
    let model: LightingRouteModel
    let color: Color

    var body: some View {
        Label(model.connectionState.displayText, systemImage: model.connectionState.symbolName)
            .font(.footnote.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(color.opacity(0.14), in: Capsule())
            .accessibilityIdentifier("lighting.connection-state")
    }
}

private struct LightingColorControls: View {
    let model: LightingRouteModel
    @Binding var hue: Double
    @Binding var saturation: Double

    var body: some View {
        LightingCard {
            LightingPowerToggle(model: model)
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("Solid color").font(.headline)
                    Spacer()
                    Button(action: setWhite) {
                        Image(systemName: "eyedropper")
                    }
                    .buttonStyle(.bordered)
                    .disabled(!model.isReady)
                    .accessibilityLabel("Set white")
                    .accessibilityIdentifier("lighting.color-picker.reset")
                }
                LightingColorWheel(hue: $hue, saturation: $saturation, onUpdate: updateColor)
                    .frame(maxWidth: .infinity)
                    .opacity(model.isReady ? 1 : 0.45)
                    .accessibilityIdentifier("lighting.color-wheel")
            }
        }
    }

    private func setWhite() {
        model.setSolidColor(red: 255, green: 255, blue: 255)
        hue = 0
        saturation = 0
    }

    private func updateColor(red: UInt8, green: UInt8, blue: UInt8, isFinal: Bool) {
        guard model.isReady else { return }
        if isFinal {
            model.setSolidColor(red: red, green: green, blue: blue)
        } else {
            model.previewSolidColor(red: red, green: green, blue: blue)
        }
    }
}

private struct LightingBrightnessControl: View {
    @Binding var brightness: Double
    let isEnabled: Bool
    let onCommit: () -> Void

    var body: some View {
        LightingCard {
            HStack {
                Text("Brightness").font(.headline)
                Spacer()
                Text("\(Int(brightness))%")
                    .monospacedDigit()
                    .foregroundStyle(PevColors.muted)
            }
            HStack(spacing: 10) {
                Image(systemName: "sun.min")
                    .foregroundStyle(PevColors.muted)
                    .accessibilityHidden(true)
                Slider(value: $brightness, in: 0...100, step: 1) { editing in
                    if !editing, isEnabled { onCommit() }
                }
                .disabled(!isEnabled)
                .tint(PevColors.primaryText)
                .accessibilityIdentifier("lighting.brightness")
                .accessibilityValue("\(Int(brightness)) percent")
                Image(systemName: "sun.max")
                    .foregroundStyle(PevColors.muted)
                    .accessibilityHidden(true)
            }
        }
    }
}

private struct LightingPresetsCard: View {
    let model: LightingRouteModel
    let isEnabled: Bool
    @Binding var hue: Double
    @Binding var saturation: Double
    @State private var presetName = ""
    @FocusState private var isPresetNameFocused: Bool

    var body: some View {
        LightingCard {
            Label("Scenes & presets", systemImage: "square.stack.3d.up.fill").font(.headline)
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 12) {
                    quickColorPreset("Red", color: .red, red: 255, green: 0, blue: 0)
                    quickColorPreset("Blue", color: .blue, red: 0, green: 0, blue: 255)
                    quickColorPreset("Night", color: .black, red: 16, green: 20, blue: 32)
                }
                if model.presets.isEmpty {
                    Text("Save your color, effect, or music settings as a named scene.")
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)
                } else {
                    ForEach(model.presets, id: \.name) { preset in
                        Button(action: { apply(preset) }) {
                            VStack(alignment: .leading, spacing: 3) {
                                HStack {
                                    Text(preset.name)
                                    Spacer()
                                    Text("\(preset.requested.brightness)%")
                                        .monospacedDigit()
                                        .foregroundStyle(.secondary)
                                }
                                Text(sceneSummary(for: preset.requested))
                                    .font(.caption)
                                    .foregroundStyle(PevColors.muted)
                            }
                        }
                        .buttonStyle(.bordered)
                        .disabled(!isEnabled)
                        .accessibilityIdentifier("lighting.preset.\(preset.name)")
                    }
                }
                HStack {
                    TextField("Preset name", text: $presetName)
                        .textFieldStyle(.roundedBorder)
                        .focused($isPresetNameFocused)
                        .submitLabel(.done)
                        .onSubmit { isPresetNameFocused = false }
                    Button("Save", action: save)
                        .buttonStyle(.borderedProminent)
                        .disabled(!model.canSavePreset || presetName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        .accessibilityIdentifier("lighting.preset.save")
                }
            }
        }
    }

    private func sceneSummary(for requested: MobileMelkLightingRestoreStateDto) -> String {
        switch requested.playback {
        case let .effect(pattern, speed):
            return "\(LightingPatternCatalog.name(for: Int(pattern))) · speed \(speed)"
        case let .music(_, sensitivity):
            return "Controller music · sensitivity \(sensitivity)%"
        case .solid, nil:
            return "Solid RGB · \(requested.brightness)%"
        }
    }

    private func quickColorPreset(
        _ title: String,
        color: Color,
        red: UInt8,
        green: UInt8,
        blue: UInt8
    ) -> some View {
        Button {
            model.setSolidColor(red: red, green: green, blue: blue)
            updateColorSelection()
        } label: {
            VStack(spacing: 6) {
                Circle()
                    .fill(color)
                    .frame(width: 42, height: 42)
                    .overlay(Circle().stroke(PevColors.cardStroke, lineWidth: 1))
                Text(title).font(.caption)
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
        .accessibilityLabel("Preset \(title)")
        .accessibilityIdentifier("lighting.quick-preset.\(title.lowercased())")
    }

    private func apply(_ preset: MobileRgbLightingPresetDto) {
        model.applyPreset(preset)
        updateColorSelection()
    }

    private func save() {
        guard model.savePreset(named: presetName) else { return }
        presetName = ""
        isPresetNameFocused = false
    }

    private func updateColorSelection() {
        let selection = lightingColorSelection(
            red: model.requestedRed,
            green: model.requestedGreen,
            blue: model.requestedBlue
        )
        hue = selection.hue
        saturation = selection.saturation
    }
}

private struct LightingErrorBanner: View {
    let error: String?

    @ViewBuilder
    var body: some View {
        if let error {
            Label(error, systemImage: "exclamationmark.triangle")
                .font(.footnote)
                .foregroundStyle(PevColors.orange)
                .accessibilityIdentifier("lighting.control-error")
        }
    }
}

private struct LightingCard<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            content
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevDashboardCardBackground(cornerRadius: 20))
    }
}

private struct LightingColorWheel: View {
    @Binding var hue: Double
    @Binding var saturation: Double
    let onUpdate: (UInt8, UInt8, UInt8, Bool) -> Void

    var body: some View {
        GeometryReader { proxy in
            let size = min(proxy.size.width, proxy.size.height)
            let radius = size / 2
            let pointerRadius = max(0, radius - 14) * saturation
            let pointerAngle = hue * 2 * .pi
            let pointerX = radius + cos(pointerAngle) * pointerRadius
            let pointerY = radius + sin(pointerAngle) * pointerRadius

            ZStack {
                Circle()
                    .fill(AngularGradient(
                        gradient: Gradient(colors: [
                            .red, .yellow, .green, .cyan, .blue, .purple, .red,
                        ]),
                        center: .center
                    ))
                Circle()
                    .fill(RadialGradient(
                        colors: [.white, .white.opacity(0)],
                        center: .center,
                        startRadius: 0,
                        endRadius: radius
                    ))
                Circle()
                    .stroke(PevColors.cardStroke, lineWidth: 1)
                Circle()
                    .fill(Color(hue: hue, saturation: saturation, brightness: 1))
                    .frame(width: size * 0.44, height: size * 0.44)
                    .overlay(Circle().stroke(.white.opacity(0.35), lineWidth: 1))
                Circle()
                    .fill(.white)
                    .frame(width: 28, height: 28)
                    .overlay(Circle().stroke(.black.opacity(0.5), lineWidth: 2))
                    .position(x: pointerX, y: pointerY)
            }
            .frame(width: size, height: size)
            .contentShape(Circle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { value in
                update(at: value.location, in: size, isFinal: false)
            }.onEnded { value in
                update(at: value.location, in: size, isFinal: true)
            })
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Solid color")
            .accessibilityValue("Hue \(Int(hue * 360)) degrees, saturation \(Int(saturation * 100)) percent")
            .accessibilityHint("Drag around the color wheel to choose a color")
            .accessibilityIdentifier("lighting.color-wheel.control")
        }
        .aspectRatio(1, contentMode: .fit)
        .frame(maxWidth: 300)
        .frame(maxWidth: .infinity)
    }

    private func update(at location: CGPoint, in size: CGFloat, isFinal: Bool) {
        let center = CGPoint(x: size / 2, y: size / 2)
        let dx = location.x - center.x
        let dy = location.y - center.y
        let radius = max(1, size / 2 - 14)
        let distance = min(radius, hypot(dx, dy))
        saturation = max(0, min(1, distance / radius))
        var angle = atan2(dy, dx) / (2 * .pi)
        if angle < 0 { angle += 1 }
        hue = angle
        let rgb = Self.rgb(hue: hue, saturation: saturation)
        onUpdate(rgb.red, rgb.green, rgb.blue, isFinal)
    }

    private static func rgb(hue: Double, saturation: Double) -> (red: UInt8, green: UInt8, blue: UInt8) {
        let scaled = hue * 6
        let sector = Int(scaled.rounded(.down)) % 6
        let fraction = scaled - floor(scaled)
        let value = 1.0
        let p = value * (1 - saturation)
        let q = value * (1 - fraction * saturation)
        let t = value * (1 - (1 - fraction) * saturation)
        let channels: (Double, Double, Double) = switch sector {
        case 0: (value, t, p)
        case 1: (q, value, p)
        case 2: (p, value, t)
        case 3: (p, q, value)
        case 4: (t, p, value)
        default: (value, p, q)
        }
        return (
            UInt8((channels.0 * 255).rounded()),
            UInt8((channels.1 * 255).rounded()),
            UInt8((channels.2 * 255).rounded())
        )
    }
}

private struct LightingPairingSheet: View {
    let model: LightingRouteModel
    let rideModel: CutoutAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var accessoryAlias = ""
    @State private var vehicleIdentifier = ""
    @State private var showsForgetConfirmation = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    pairingStatusCard
                    Button {
                        if model.canReconnect { model.reconnect() } else { model.start() }
                    } label: {
                        Label(model.isReady ? "Connected" : "Connect", systemImage: "link")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(model.isReady)
                    .accessibilityIdentifier("lighting.pairing.connect")

                    Toggle(
                        "Restore last lighting settings",
                        isOn: Binding(
                            get: { model.restoreEnabled },
                            set: { model.setRestoreEnabled($0) }
                        )
                    )
                    .tint(PevColors.cyan)
                    .accessibilityIdentifier("lighting.restore-toggle")
                    Text("Re-apply your last color, effect, brightness, and power setting when this same accessory reconnects.")
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)

                    metadataCard
                    warningCard

                    if model.canEditMetadata {
                        Button("Forget accessory", role: .destructive) {
                            showsForgetConfirmation = true
                        }
                        .buttonStyle(.bordered)
                        .frame(maxWidth: .infinity)
                        .accessibilityIdentifier("lighting.forget-accessory")
                    }
                }
                .padding(20)
            }
            .background(PevColors.pageBackground.ignoresSafeArea())
            .navigationTitle("Add lighting")
#if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
#endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Back") { dismiss() }
                }
            }
        }
        .task {
            accessoryAlias = model.accessoryAlias ?? ""
            vehicleIdentifier = model.vehicleIdentifier ?? ""
        }
        .confirmationDialog(
            "Forget this RGB accessory?",
            isPresented: $showsForgetConfirmation,
            titleVisibility: .visible
        ) {
            Button("Forget accessory", role: .destructive) {
                model.forgetAccessory()
                dismiss()
            }
        } message: {
            Text("Its alias, vehicle association, presets, and automatic restore preference will be removed.")
        }
    }

    private var pairingStatusCard: some View {
        LightingCard {
            HStack(spacing: 12) {
                Image(systemName: "lightbulb.led.fill")
                    .foregroundStyle(PevColors.cyan)
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.peripheralName ?? "Scanning")
                        .font(.headline)
                    Text(model.connectionState == .scanning ? "Looking for nearby accessories…" : model.connectionState.displayText)
                        .font(.subheadline)
                        .foregroundStyle(PevColors.muted)
                }
                Spacer()
                connectionPill
            }
            Text("Lighting stays connected independently of your ride.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
        }
    }

    private var metadataCard: some View {
        LightingCard {
            Text("Alias (optional)")
                .font(.headline)
            TextField("Help identify this accessory", text: $accessoryAlias)
                .textFieldStyle(.roundedBorder)
                .accessibilityIdentifier("lighting.accessory-alias")

            HStack(spacing: 10) {
                TextField("Installed vehicle identifier", text: $vehicleIdentifier)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("lighting.vehicle-association")
                if let selectedRideIdentifier = rideModel.selectedRideIdentifier {
                    Button("Use current ride") {
                        vehicleIdentifier = selectedRideIdentifier
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("lighting.use-current-ride")
                }
            }

            Button("Save details") {
                model.saveAccessoryMetadata(alias: accessoryAlias, vehicleIdentifier: vehicleIdentifier)
            }
            .buttonStyle(.bordered)
            .frame(maxWidth: .infinity)
            .disabled(!model.canEditMetadata)
            .accessibilityIdentifier("lighting.save-accessory-details")
        }
    }

    private var warningCard: some View {
        LightingCard {
            Label("Competing client", systemImage: "exclamationmark.triangle")
                .foregroundStyle(PevColors.yellow)
            Text("If another app is connected to this controller, disconnect it there before connecting in Cutout.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
        }
    }

    private var connectionPill: some View {
        Label(model.connectionState.displayText, systemImage: model.connectionState.symbolName)
            .font(.caption.weight(.semibold))
            .foregroundStyle(model.connectionState == .ready ? PevColors.green : PevColors.yellow)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background((model.connectionState == .ready ? PevColors.green : PevColors.yellow).opacity(0.14), in: Capsule())
    }
}

