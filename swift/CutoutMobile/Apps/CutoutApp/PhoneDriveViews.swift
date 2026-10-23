import CutoutMobile
import CutoutMobileFFI
import SwiftUI

#if os(iOS)
    import UIKit
#endif

/// Native geometry only. Rust retains initial-region ownership and contact policy.
enum PhoneDriveTouchGeometry {
    static let buttonGap: CGFloat = 12

    static func region(at point: CGPoint, size: CGSize, mode: MobilePhoneControlModeDto) -> MobilePhoneDriveActionDto? {
        guard point.x.isFinite, point.y.isFinite, size.width.isFinite, size.height.isFinite,
            size.width > 0, size.height > 0,
            point.x >= 0, point.y >= 0, point.x < size.width, point.y < size.height
        else { return nil }
        switch mode {
        case .disabled:
            return nil
        case .area:
            return point.x < size.width / 2 ? .brake : .throttle
        case .buttons:
            let middle = size.width / 2
            if point.x < middle - buttonGap / 2 { return .brake }
            if point.x >= middle + buttonGap / 2 { return .throttle }
            return nil
        }
    }
}

struct PhoneDriveRideSurface<Content: View>: View {
    let model: PhoneDriveModel
    let content: Content
    @Environment(\.musicCompactPlayerFrame) private var musicCompactPlayerFrame

    init(model: PhoneDriveModel, @ViewBuilder content: () -> Content) {
        self.model = model
        self.content = content()
    }

    var body: some View {
        GeometryReader { geometry in
            let height = RideViewportLayout.visibleHeight(
                in: geometry, musicPlayerFrame: musicCompactPlayerFrame,
                bottomSafeAreaInset: geometry.safeAreaInsets.bottom)
            VStack(spacing: 0) {
                content
                    .allowsHitTesting(!(model.capturesInput && model.snapshot.mode == .area))
                    .frame(maxHeight: .infinity)
                if model.snapshot.mode == .buttons {
                    buttonsPanel(height: min(200, max(110, height * 0.31)))
                        .padding(.horizontal, 24)
                        .padding(.bottom, 24)
                } else if model.snapshot.mode == .area {
                    areaStatus
                        .padding(.horizontal, 24)
                        .padding(.bottom, 12)
                }
            }
            .frame(width: geometry.size.width, height: height)
            .overlay {
                if model.snapshot.mode == .area, model.capturesInput {
                    PhoneDriveTouchSurface(
                        model: model, mode: .area, acceptsInput: model.capturesInput
                    )
                    .background { areaFeedback }
                    .accessibilityElement()
                    .accessibilityLabel(localizedAppText("phone_control.area_hint"))
                    .accessibilityValue(model.feedbackText)
                    .accessibilityAction(named: localizedAppText("phone_control.brake")) { pulse(.brake) }
                    .accessibilityAction(named: localizedAppText("phone_control.throttle")) { pulse(.throttle) }
                    .accessibilityIdentifier("phone-control.area")
                }
            }
            .onChange(of: geometry.size) { _, _ in model.cancel(.layoutChanged) }
        }
        .foregroundStyle(PevDashboardColors.rideInk)
        .background(PevDashboardColors.rideCanvas.ignoresSafeArea())
        .overlay(alignment: .topLeading) {
            #if DEBUG && targetEnvironment(simulator)
                if model.isInputPreview {
                    Text("Phone input preview receipts")
                        .font(.system(size: 1))
                        .frame(width: 1, height: 1)
                        .opacity(0.01)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel("Phone input preview receipts")
                        .accessibilityValue(model.previewReceiptValue)
                        .accessibilityIdentifier("phone-control.preview-receipts")
                        .allowsHitTesting(false)
                }
            #endif
        }
        .onDisappear { model.cancel(.navigation) }
    }

    private func buttonsPanel(height: CGFloat) -> some View {
        HStack(spacing: PhoneDriveTouchGeometry.buttonGap) {
            holdButton(.brake)
            holdButton(.throttle)
        }
        .frame(height: height)
        .overlay {
            if model.snapshot.mode == .buttons {
                PhoneDriveTouchSurface(
                    model: model, mode: .buttons, acceptsInput: model.capturesInput
                )
                .accessibilityHidden(true)
            }
        }
        .accessibilityIdentifier("phone-control.buttons")
    }

    private func holdButton(_ action: MobilePhoneDriveActionDto) -> some View {
        let throttle = action == .throttle
        let pressed: Bool = {
            if case .pressed(let held) = model.snapshot.phase { return held == action }
            return false
        }()
        return Button {
            // Discrete assistive/keyboard activation is a complete contact pulse.
            // It never becomes a persistent held command after focus moves.
            pulse(action)
        } label: {
            VStack(spacing: 5) {
                Image(systemName: throttle ? "arrow.up" : "equal")
                    .font(.system(size: 27, weight: .semibold))
                Text(localizedAppText(throttle ? "phone_control.throttle" : "phone_control.brake"))
                    .font(.system(size: 29, weight: .bold))
                    .tracking(-0.8)
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                Text(localizedAppText(pressed ? "phone_control.held" : "phone_control.hold"))
                    .font(.system(size: 13))
                    .opacity(0.7)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .foregroundStyle(throttle ? PevDashboardColors.rideAccent : PevDashboardColors.rideInk)
            .background(
                throttle ? PevDashboardColors.rideControlFill : PevDashboardColors.rideCanvas,
                in: RoundedRectangle(cornerRadius: 24)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 24)
                    .strokeBorder(
                        throttle ? PevDashboardColors.rideControlFill : PevDashboardColors.rideInk,
                        lineWidth: 2
                    )
                if pressed {
                    RoundedRectangle(cornerRadius: 27)
                        .strokeBorder(PevDashboardColors.rideAccent, lineWidth: 3)
                        .padding(-4)
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: 24))
        }
        .buttonStyle(.plain)
        .keyboardShortcut(throttle ? .rightArrow : .leftArrow, modifiers: [])
        .disabled(!model.capturesInput)
        .accessibilityIdentifier(throttle ? "phone-control.throttle" : "phone-control.brake")
    }

    private var areaStatus: some View {
        VStack(spacing: 6) {
            Text(localizedAppText("phone_control.area_hint"))
                .font(.caption)
                .foregroundStyle(PevDashboardColors.rideMuted)
                .lineLimit(1)
                .minimumScaleFactor(0.45)
            if case .pressed = model.snapshot.phase {
                Text(model.feedbackText)
                    .font(.caption.weight(.semibold))
                    .accessibilityIdentifier("phone-control.feedback")
            }
        }
    }

    private func pulse(_ action: MobilePhoneDriveActionDto) {
        guard model.capturesInput, let generation = model.snapshot.generation else { return }
        #if os(iOS)
            model.nativeContacts.discreteAction(action)
        #else
            model.contacts(.single(contactId: UInt64.max, region: action), generation: generation)
            model.contacts(.released, generation: generation)
        #endif
    }

    private var areaFeedback: some View {
        HStack(spacing: 0) {
            PevDashboardColors.rideAccent.opacity(isPressed(.brake) ? 0.12 : 0)
            PevDashboardColors.rideAccent.opacity(isPressed(.throttle) ? 0.12 : 0)
        }
        .allowsHitTesting(false)
    }

    private func isPressed(_ action: MobilePhoneDriveActionDto) -> Bool {
        if case .pressed(let held) = model.snapshot.phase { return held == action }
        return false
    }

}

struct PhoneDriveSettingsSheet: View {
    let model: PhoneDriveModel
    let requestActivation: (PhoneDriveModel.ActivationRequest) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker(
                        localizedAppText("phone_control.input_mode"),
                        selection: Binding(
                            get: { model.settingsMode },
                            set: { model.updateSettingsMode($0) }
                        )
                    ) {
                        ForEach([MobilePhoneControlModeDto.disabled, .buttons, .area], id: \.self) { mode in
                            Text(mode.phoneControlTitle).tag(mode)
                        }
                    }
                    .disabled(model.settingsBusy || !model.settingsLoaded)
                    .accessibilityIdentifier("ride.phone-control.mode")
                    phoneDriveCurrentField(
                        "phone_control.throttle_current",
                        text: Binding(
                            get: { model.throttleAmpsText },
                            set: { model.throttleAmpsText = $0 }
                        ),
                        identifier: "ride.phone-control.throttle-current"
                    )
                    phoneDriveCurrentField(
                        "phone_control.brake_current",
                        text: Binding(
                            get: { model.brakeAmpsText },
                            set: { model.brakeAmpsText = $0 }
                        ),
                        identifier: "ride.phone-control.brake-current"
                    )
                    Button(localizedAppText("phone_control.save_limits")) {
                        Task { _ = await model.saveSettings() }
                    }
                    .disabled(model.settingsBusy || !model.settingsLoaded)
                    .accessibilityIdentifier("ride.phone-control.save")
                    if model.settingsMode != .disabled, model.settingsLoaded {
                        Button(localizedAppText("phone_control.arm")) {
                            Task {
                                if let request = await model.saveSettingsAndRequestActivation() {
                                    requestActivation(request)
                                }
                            }
                        }
                        .disabled(model.settingsBusy)
                        .accessibilityIdentifier("ride.phone-control.activate")
                    }
                    if model.activationError {
                        Text(localizedAppText("phone_control.activation_failed"))
                            .foregroundStyle(.red)
                            .accessibilityIdentifier("ride.phone-control.activation-error")
                    }
                    if model.limitsError {
                        Text(localizedAppText("phone_control.limits_failed"))
                            .foregroundStyle(.red)
                            .accessibilityIdentifier("ride.phone-control.limits-error")
                    }
                    if model.settingsError {
                        Text(
                            localizedAppText(
                                model.settingsLoaded ? "phone_control.save_failed" : "phone_control.load_failed")
                        )
                        .foregroundStyle(.red)
                        .accessibilityIdentifier("ride.phone-control.error")
                    }
                    if !model.settingsLoaded && model.settingsBusy {
                        ProgressView(localizedAppText("phone_control.input_mode"))
                    } else if !model.settingsLoaded {
                        Button(localizedAppText("phone_control.retry")) {
                            Task { await model.loadSettings() }
                        }
                        .disabled(model.settingsBusy)
                    }
                }
                Section {
                    Text(localizedAppText("phone_control.limitations"))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle(localizedAppText("phone_control.title"))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(localizedAppText("music.done")) { dismiss() }
                        .accessibilityIdentifier("ride.phone-control.done")
                }
            }
        }
        .task { await model.loadSettings() }
        .accessibilityIdentifier("ride.phone-control.settings-sheet")
    }

    private func phoneDriveCurrentField(
        _ labelKey: String,
        text: Binding<String>,
        identifier: String
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(localizedAppText(labelKey))
            TextField(localizedAppText(labelKey), text: text)
                .labelsHidden()
                .frame(minHeight: 44)
                .disabled(model.settingsBusy || !model.settingsLoaded)
                .autocorrectionDisabled()
                #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .keyboardType(.numbersAndPunctuation)
                #endif
                .accessibilityLabel(localizedAppText(labelKey))
                .accessibilityIdentifier(identifier)
        }
    }
}

#if os(iOS)
    /// One passive observer remains attached to this app's window while input
    /// surfaces change. Only actual framework observations can establish all-up.
    @MainActor
    final class PhoneDriveNativeContactObserver: NSObject, UIGestureRecognizerDelegate {
        var now: () -> UInt64 = { 0 }
        var currentGeneration: () -> UInt64? = { nil }
        var observe: ((MobilePhoneDriveContactsDto, UInt64, UInt64) -> Void)?
        var cancel: ((MobilePhoneDriveCancelReasonDto) -> Void)?
        private weak var window: UIWindow?
        private weak var inputView: PhoneDriveInputView?
        private var recognizer: PhoneDriveWindowGestureRecognizer?
        private var contacts: [ObjectIdentifier: (id: UInt64, touch: UITouch, input: Bool)] = [:]
        private var nextContactID: UInt64 = 1
        private var contactGeneration: UInt64?
        private var hasAttachedWindow = false
        private var observationIsComplete = false
        private var heartbeat: Task<Void, Never>?

        deinit { heartbeat?.cancel() }

        func attach(to newWindow: UIWindow?) {
            guard window !== newWindow else { return }
            if let recognizer { window?.removeGestureRecognizer(recognizer) }
            heartbeat?.cancel()
            heartbeat = nil
            if window != nil { cancel?(.sceneInactive) }
            contacts.removeAll()
            contactGeneration = nil
            observationIsComplete = false
            window = newWindow
            guard let newWindow else {
                recognizer = nil
                return
            }
            if !hasAttachedWindow {
                // The first app-owned window establishes the cold framework
                // epoch. Once observation is lost, reattachment stays unknown.
                observationIsComplete = true
                hasAttachedWindow = true
            }
            let recognizer = PhoneDriveWindowGestureRecognizer(owner: self)
            recognizer.cancelsTouchesInView = false
            recognizer.delaysTouchesBegan = false
            recognizer.delaysTouchesEnded = false
            recognizer.delegate = self
            newWindow.addGestureRecognizer(recognizer)
            self.recognizer = recognizer
        }

        fileprivate func register(_ view: PhoneDriveInputView) { inputView = view }

        fileprivate func unregister(_ view: PhoneDriveInputView) {
            guard inputView === view else { return }
            inputView = nil
            cancel?(.navigation)
        }

        fileprivate func observeEvent(_ event: UIEvent, cancelled: Bool) {
            if cancelled {
                observationIsComplete = false
                heartbeat?.cancel()
                heartbeat = nil
                cancel?(.touchCancelled)
                return
            }
            guard let touches = event.allTouches else {
                observationIsComplete = false
                cancel?(.touchCancelled)
                return
            }
            let active = touches.filter {
                $0.window === window && ($0.phase == .began || $0.phase == .moved || $0.phase == .stationary)
            }
            if contacts.isEmpty, !active.isEmpty { contactGeneration = currentGeneration() }
            let previous = contacts
            contacts.removeAll(keepingCapacity: true)
            for touch in active {
                let key = ObjectIdentifier(touch)
                if let retained = previous[key] {
                    contacts[key] = retained
                    continue
                }
                guard nextContactID < UInt64.max else {
                    cancel?(.generationExhausted)
                    return
                }
                let input =
                    touch.phase == .began
                    && (inputView.map { $0.acceptsInput && $0.region(for: touch) != nil } ?? false)
                contacts[key] = (nextContactID, touch, input)
                nextContactID += 1
            }
            observationIsComplete = true
            let observedAtMs = now()
            if contacts.isEmpty {
                heartbeat?.cancel()
                heartbeat = nil
                contactGeneration = nil
                reobserveAllUp()
            } else {
                publish(observedAtMs: observedAtMs)
                startHeartbeat()
            }
        }

        func reobserveAllUp() {
            guard sampleKnownAllUp(),
                let generation = currentGeneration()
            else { return }
            // A continuously observed empty contact set can be sampled at the
            // completed keyboard/accessibility action; held intent is never cached.
            observe?(.released, generation, now())
        }

        func discreteAction(_ action: MobilePhoneDriveActionDto) {
            guard let generation = currentGeneration() else { return }
            guard sampleKnownAllUp() else {
                observe?(.multiple, generation, now())
                return
            }
            let observedAtMs = now()
            observe?(.single(contactId: UInt64.max, region: action), generation, observedAtMs)
            observe?(.released, generation, observedAtMs)
        }

        private func sampleKnownAllUp() -> Bool {
            guard window != nil, observationIsComplete else { return false }
            // UIKit can invoke a completed Button action before this passive
            // recognizer's end callback. Read the retained framework objects now.
            if contacts.values.contains(where: { $0.touch.phase == .cancelled }) {
                observationIsComplete = false
                heartbeat?.cancel()
                heartbeat = nil
                cancel?(.touchCancelled)
                return false
            }
            contacts = contacts.filter { $0.value.touch.phase != .ended }
            guard contacts.isEmpty else { return false }
            contactGeneration = nil
            heartbeat?.cancel()
            heartbeat = nil
            return true
        }

        private func startHeartbeat() {
            guard heartbeat == nil else { return }
            heartbeat = Task { [weak self] in
                while !Task.isCancelled {
                    do { try await Task.sleep(for: .milliseconds(50)) } catch { return }
                    guard !Task.isCancelled, let self, !self.contacts.isEmpty else { return }
                    guard
                        self.contacts.values.allSatisfy({
                            $0.touch.phase == .began || $0.touch.phase == .moved || $0.touch.phase == .stationary
                        })
                    else {
                        self.observationIsComplete = false
                        self.cancel?(.touchCancelled)
                        return
                    }
                    self.publish(observedAtMs: self.now())
                }
            }
        }

        private func publish(observedAtMs: UInt64) {
            guard let generation = contactGeneration else { return }
            let observation: MobilePhoneDriveContactsDto
            if contacts.count > 1 {
                observation = .multiple
            } else if let contact = contacts.values.first {
                observation = .single(
                    contactId: contact.id,
                    region: contact.input ? inputView?.region(for: contact.touch) : nil)
            } else {
                return
            }
            observe?(observation, generation, observedAtMs)
        }

        func gestureRecognizer(
            _ gestureRecognizer: UIGestureRecognizer,
            shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
        ) -> Bool { true }
    }

    private final class PhoneDriveWindowGestureRecognizer: UIGestureRecognizer {
        private weak var owner: PhoneDriveNativeContactObserver?
        init(owner: PhoneDriveNativeContactObserver) {
            self.owner = owner
            super.init(target: nil, action: nil)
        }
        required init?(coder: NSCoder) { return nil }
        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
            owner?.observeEvent(event, cancelled: false)
        }
        override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
            owner?.observeEvent(event, cancelled: false)
        }
        override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
            owner?.observeEvent(event, cancelled: false)
            if event.allTouches?.allSatisfy({ $0.phase == .ended || $0.phase == .cancelled }) == true {
                state = .failed
            }
        }
        override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
            owner?.observeEvent(event, cancelled: true)
            state = .failed
        }
        override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
        override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    }

    struct PhoneDriveWindowContactObservations: UIViewRepresentable {
        let model: PhoneDriveModel
        func makeUIView(context: Context) -> PhoneDriveWindowProbe {
            let view = PhoneDriveWindowProbe()
            view.observer = model.nativeContacts
            view.isUserInteractionEnabled = false
            return view
        }
        func updateUIView(_ view: PhoneDriveWindowProbe, context: Context) { view.observer = model.nativeContacts }
        static func dismantleUIView(_ view: PhoneDriveWindowProbe, coordinator: ()) { view.observer?.attach(to: nil) }
    }

    final class PhoneDriveWindowProbe: UIView {
        weak var observer: PhoneDriveNativeContactObserver?
        override func didMoveToWindow() {
            super.didMoveToWindow()
            observer?.attach(to: window)
        }
    }

    private struct PhoneDriveTouchSurface: UIViewRepresentable {
        let model: PhoneDriveModel
        let mode: MobilePhoneControlModeDto
        let acceptsInput: Bool
        func makeUIView(context: Context) -> PhoneDriveInputView {
            let view = PhoneDriveInputView()
            view.backgroundColor = .clear
            view.isMultipleTouchEnabled = true
            view.observer = model.nativeContacts
            return view
        }
        func updateUIView(_ view: PhoneDriveInputView, context: Context) {
            view.mode = mode
            view.acceptsInput = acceptsInput
            model.nativeContacts.register(view)
        }
        static func dismantleUIView(_ view: PhoneDriveInputView, coordinator: ()) { view.observer?.unregister(view) }
    }

    private final class PhoneDriveInputView: UIView {
        weak var observer: PhoneDriveNativeContactObserver?
        var mode = MobilePhoneControlModeDto.disabled
        var acceptsInput = false
        private var previousSize: CGSize?
        override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
            acceptsInput && PhoneDriveTouchGeometry.region(at: point, size: bounds.size, mode: mode) != nil
        }
        func region(for touch: UITouch) -> MobilePhoneDriveActionDto? {
            PhoneDriveTouchGeometry.region(at: touch.location(in: self), size: bounds.size, mode: mode)
        }
        override func layoutSubviews() {
            super.layoutSubviews()
            if let previousSize, previousSize != bounds.size { observer?.cancel?(.layoutChanged) }
            previousSize = bounds.size
        }
    }
#else
    struct PhoneDriveWindowContactObservations: View {
        let model: PhoneDriveModel
        var body: some View { Color.clear.allowsHitTesting(false) }
    }
    private struct PhoneDriveTouchSurface: View {
        let model: PhoneDriveModel
        let mode: MobilePhoneControlModeDto
        let acceptsInput: Bool
        var body: some View { Color.clear.allowsHitTesting(false) }
    }
#endif
