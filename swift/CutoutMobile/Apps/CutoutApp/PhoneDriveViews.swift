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
            let frame = geometry.frame(in: .global)
            let safeBottom = frame.maxY - geometry.safeAreaInsets.bottom
            let bottom = min(safeBottom, musicCompactPlayerFrame?.minY ?? safeBottom)
            let height = max(0, bottom - frame.minY)
            VStack(spacing: 0) {
                content
                    .allowsHitTesting(!(model.capturesInput && model.snapshot.mode == .area))
                    .frame(maxHeight: .infinity)
                if model.snapshot.mode == .buttons {
                    buttonsPanel(height: min(190, max(110, height * 0.29)))
                        .padding(.horizontal, 24)
                        .padding(.bottom, 12)
                } else if model.snapshot.mode == .area {
                    areaStatus
                        .padding(.horizontal, 24)
                        .padding(.bottom, 12)
                }
            }
            .frame(width: geometry.size.width, height: height)
            .overlay {
                if model.capturesInput, model.snapshot.mode == .area,
                    let generation = model.snapshot.generation
                {
                    PhoneDriveTouchSurface(
                        mode: .area, generation: generation,
                        observe: model.contacts, cancel: model.cancel
                    )
                    .background { areaFeedback }
                    .accessibilityElement()
                    .accessibilityLabel(localizedAppText("phone_control.title"))
                    .accessibilityValue(model.feedbackText)
                    .accessibilityAction(named: localizedAppText("phone_control.brake")) { pulse(.brake) }
                    .accessibilityAction(named: localizedAppText("phone_control.throttle")) { pulse(.throttle) }
                    .accessibilityIdentifier("phone-control.area")
                }
            }
            .onChange(of: geometry.size) { _, _ in model.cancel(.layoutChanged) }
        }
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
        VStack(spacing: 8) {
            HStack {
                Text(localizedAppText("phone_control.title")).font(.caption.weight(.semibold))
                Spacer()
                activationStatus
            }
            HStack(spacing: PhoneDriveTouchGeometry.buttonGap) {
                holdButton(.brake)
                holdButton(.throttle)
            }
            .frame(height: height)
            .overlay {
                if model.capturesInput, let generation = model.snapshot.generation {
                    PhoneDriveTouchSurface(
                        mode: .buttons, generation: generation,
                        observe: model.contacts, cancel: model.cancel
                    )
                    .accessibilityHidden(true)
                }
            }
            statusText
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
                    .font(.title2.weight(.semibold))
                Text(localizedAppText(throttle ? "phone_control.throttle" : "phone_control.brake"))
                    .font(.title2.weight(.bold))
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                Text(localizedAppText(pressed ? "phone_control.held" : "phone_control.hold"))
                    .font(.caption)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .foregroundStyle(throttle ? Color.yellow : PevColors.primaryText)
            .background(throttle ? Color.black : PevColors.pageBackground, in: RoundedRectangle(cornerRadius: 24))
            .overlay {
                RoundedRectangle(cornerRadius: 24).strokeBorder(PevColors.primaryText, lineWidth: pressed ? 4 : 2)
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
            HStack {
                Text(localizedAppText("phone_control.area_hint")).font(.caption)
                Spacer(minLength: 4)
                activationStatus
            }
            if model.isInputPreview, !model.feedbackText.isEmpty {
                Text(model.feedbackText)
                    .font(.caption.weight(.semibold))
                    .accessibilityIdentifier("phone-control.feedback")
            }
            statusText
        }
    }

    private func pulse(_ action: MobilePhoneDriveActionDto) {
        guard model.capturesInput, let generation = model.snapshot.generation else { return }
        model.contacts(.single(contactId: UInt64.max, region: action), generation: generation)
        model.contacts(.released, generation: generation)
    }

    private var areaFeedback: some View {
        HStack(spacing: 0) {
            Color.yellow.opacity(isPressed(.brake) ? 0.12 : 0)
            Color.yellow.opacity(isPressed(.throttle) ? 0.12 : 0)
        }
        .allowsHitTesting(false)
    }

    private func isPressed(_ action: MobilePhoneDriveActionDto) -> Bool {
        if case .pressed(let held) = model.snapshot.phase { return held == action }
        return false
    }

    @ViewBuilder
    private var activationStatus: some View {
        if !model.isInputPreview {
            Button(localizedAppText("phone_control.arm")) {}
                .disabled(true)
                .font(.caption)
                .accessibilityIdentifier("phone-control.activate")
        }
    }

    private var statusText: some View {
        Text(model.isInputPreview ? localizedAppText("phone_control.preview") : model.unavailableText)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityIdentifier("phone-control.status")
    }
}

#if os(iOS)
    /// Reports complete native contact observations; it contains no drive policy.
    private struct PhoneDriveTouchSurface: UIViewRepresentable {
        let mode: MobilePhoneControlModeDto
        let generation: UInt64
        let observe: (MobilePhoneDriveContactsDto, UInt64) -> Void
        let cancel: (MobilePhoneDriveCancelReasonDto) -> Void

        func makeUIView(context: Context) -> PhoneDriveTouchView {
            let view = PhoneDriveTouchView()
            view.backgroundColor = .clear
            view.isMultipleTouchEnabled = true
            return view
        }

        func updateUIView(_ view: PhoneDriveTouchView, context: Context) {
            view.mode = mode
            view.generation = generation
            view.observe = observe
            view.cancel = cancel
            view.reportReleasedWhenIdle()
        }

        static func dismantleUIView(_ view: PhoneDriveTouchView, coordinator: ()) {
            view.cancel?(.navigation)
        }
    }

    private final class PhoneDriveTouchView: UIView {
        var mode = MobilePhoneControlModeDto.disabled
        var generation: UInt64 = 0
        var observe: ((MobilePhoneDriveContactsDto, UInt64) -> Void)?
        var cancel: ((MobilePhoneDriveCancelReasonDto) -> Void)?
        private var contacts: [ObjectIdentifier: (id: UInt64, point: CGPoint)] = [:]
        private var nextContactID: UInt64 = 1
        private var contactGeneration: UInt64?
        private var reportedIdleGeneration: UInt64?
        private var previousSize: CGSize?

        override func layoutSubviews() {
            super.layoutSubviews()
            if let previousSize, previousSize != bounds.size { cancel?(.layoutChanged) }
            previousSize = bounds.size
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            if window == nil { cancel?(.sceneInactive) }
        }

        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
            if contacts.isEmpty { contactGeneration = generation }
            reportedIdleGeneration = nil
            for touch in touches {
                guard nextContactID < UInt64.max else {
                    cancel?(.generationExhausted)
                    return
                }
                contacts[ObjectIdentifier(touch)] = (nextContactID, touch.location(in: self))
                nextContactID += 1
            }
            publish()
        }

        override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
            for touch in touches {
                let key = ObjectIdentifier(touch)
                guard let contact = contacts[key] else { continue }
                contacts[key] = (contact.id, touch.location(in: self))
            }
            publish()
        }

        override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
            for touch in touches { contacts.removeValue(forKey: ObjectIdentifier(touch)) }
            publish()
            if contacts.isEmpty {
                contactGeneration = nil
                reportReleasedWhenIdle()
            }
        }

        override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
            cancel?(.touchCancelled)
            for touch in touches { contacts.removeValue(forKey: ObjectIdentifier(touch)) }
            publish()
            if contacts.isEmpty {
                contactGeneration = nil
                reportReleasedWhenIdle()
            }
        }

        func reportReleasedWhenIdle() {
            guard contacts.isEmpty, reportedIdleGeneration != generation else { return }
            let observedGeneration = generation
            reportedIdleGeneration = observedGeneration
            DispatchQueue.main.async { [weak self] in
                guard let self, self.contacts.isEmpty, self.generation == observedGeneration else { return }
                self.observe?(.released, observedGeneration)
            }
        }

        private func publish() {
            guard let contactGeneration else { return }
            let observation: MobilePhoneDriveContactsDto
            if contacts.count > 1 {
                observation = .multiple
            } else if let contact = contacts.values.first {
                observation = .single(
                    contactId: contact.id,
                    region: PhoneDriveTouchGeometry.region(at: contact.point, size: bounds.size, mode: mode)
                )
            } else {
                observation = .released
            }
            observe?(observation, contactGeneration)
        }
    }
#else
    private struct PhoneDriveTouchSurface: View {
        let mode: MobilePhoneControlModeDto
        let generation: UInt64
        let observe: (MobilePhoneDriveContactsDto, UInt64) -> Void
        let cancel: (MobilePhoneDriveCancelReasonDto) -> Void
        var body: some View { Color.clear.allowsHitTesting(false) }
    }
#endif
