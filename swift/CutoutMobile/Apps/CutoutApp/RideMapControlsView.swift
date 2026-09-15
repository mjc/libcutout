import CutoutMobile
import SwiftUI

struct RideMapControlsView: View {
    enum ControlSet: Equatable {
        case recording
        case resumable
        case terminal
        case start
    }

    let state: MobileRideMapStateDto?
    let allowedActions: [MobileRideMapActionDto]
    @Binding var isDiscardConfirmationPresented: Bool
    let pause: () -> Void
    let resume: () -> Void
    let save: () -> Void
    let stop: () -> Void
    let start: () -> Void

    static func controlSet(for state: MobileRideMapStateDto?) -> ControlSet {
        switch state {
        case .active: .recording
        case .paused, .interrupted: .resumable
        case .stopped: .terminal
        case .draft, .saved, .discarded, .imported, nil: .start
        }
    }

    var body: some View {
        switch Self.controlSet(for: state) {
        case .recording:
            RideMapAdaptiveControls {
                if allows(.pause) { pauseButton }
                if allows(.stop) { stopButton(prominent: true) }
            }
        case .resumable:
            RideMapAdaptiveControls {
                if allows(.resume) { resumeButton }
                if allows(.stop) { stopButton(prominent: false) }
                if allows(.save) { saveButton }
                if allows(.discard) { discardButton }
            }
        case .terminal:
            RideMapAdaptiveControls {
                if allows(.save) { saveButton }
                if allows(.discard) { discardButton }
            }
        case .start:
            if allows(.start) { startButton }
        }
    }

    private var pauseButton: some View {
        Button(action: pause) {
            Label(localizedAppText("ride_map.pause"), systemImage: "pause.fill")
                .frame(maxWidth: .infinity, minHeight: 48)
        }
        .buttonStyle(.borderedProminent)
        .tint(PevColors.yellow)
        .accessibilityIdentifier("ride-map.pause")
    }

    private var resumeButton: some View {
        Button(action: resume) {
            Label(localizedAppText("ride_map.resume"), systemImage: "play.fill")
                .frame(maxWidth: .infinity, minHeight: 48)
        }
        .buttonStyle(.borderedProminent)
        .tint(PevColors.yellow)
        .accessibilityIdentifier("ride-map.resume")
    }

    @ViewBuilder
    private func stopButton(prominent: Bool) -> some View {
        Button(role: .destructive, action: stop) {
            Label(localizedAppText("ride_map.stop"), systemImage: "stop.fill")
                .frame(maxWidth: .infinity, minHeight: 48)
        }
        .modifier(StopButtonStyle(prominent: prominent))
        .tint(PevColors.red)
        .accessibilityIdentifier("ride-map.stop")
    }

    private struct StopButtonStyle: ViewModifier {
        let prominent: Bool

        @ViewBuilder
        func body(content: Content) -> some View {
            if prominent {
                content.buttonStyle(.borderedProminent)
            } else {
                content.buttonStyle(.bordered)
            }
        }
    }

    private var startButton: some View {
        Button(action: start) {
            Label(
                localizedAppText(state == nil ? "ride_map.start" : "ride_map.start_new"),
                systemImage: "location.fill"
            )
            .frame(maxWidth: .infinity, minHeight: 48)
        }
        .buttonStyle(.borderedProminent)
        .tint(PevColors.yellow)
        .accessibilityIdentifier("ride-map.start")
    }

    private var saveButton: some View {
        Button(action: save) {
            Label(localizedAppText("ride_map.save"), systemImage: "checkmark.circle.fill")
                .frame(maxWidth: .infinity, minHeight: 48)
        }
        .buttonStyle(.borderedProminent)
        .accessibilityIdentifier("ride-map.save")
    }

    private var discardButton: some View {
        Button(role: .destructive) {
            isDiscardConfirmationPresented = true
        } label: {
            Label(localizedAppText("ride_map.discard"), systemImage: "trash")
                .frame(maxWidth: .infinity, minHeight: 48)
        }
        .buttonStyle(.bordered)
        .accessibilityIdentifier("ride-map.discard")
    }

    private func allows(_ action: MobileRideMapActionDto) -> Bool {
        allowedActions.contains(action)
    }
}

private struct RideMapAdaptiveControls<Content: View>: View {
    private let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) {
                content
            }
            VStack(alignment: .leading, spacing: 12) {
                content
            }
        }
    }
}
