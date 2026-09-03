import CutoutMobile
import SwiftUI

struct RideMapControlsView: View {
    let state: MobileRideMapStateDto?
    @Binding var isDiscardConfirmationPresented: Bool
    let pause: () -> Void
    let resume: () -> Void
    let save: () -> Void
    let stop: () -> Void
    let start: () -> Void

    var body: some View {
        switch state {
        case .active:
            RideMapAdaptiveControls {
                Button(action: pause) {
                    Label(localizedAppText("ride_map.pause"), systemImage: "pause.fill")
                        .frame(maxWidth: .infinity, minHeight: 48)
                }
                .buttonStyle(.borderedProminent)
                .tint(PevColors.yellow)
                .accessibilityIdentifier("ride-map.pause")
                stopButton(prominent: true)
            }
        case .paused:
            RideMapAdaptiveControls {
                Button(action: resume) {
                    Label(localizedAppText("ride_map.resume"), systemImage: "play.fill")
                        .frame(maxWidth: .infinity, minHeight: 48)
                }
                .buttonStyle(.borderedProminent)
                .tint(PevColors.yellow)
                .accessibilityIdentifier("ride-map.resume")
                stopButton(prominent: false)
            }
        case .stopped:
            RideMapAdaptiveControls {
                Button(action: save) {
                    Label(localizedAppText("ride_map.save"), systemImage: "checkmark.circle.fill")
                        .frame(maxWidth: .infinity, minHeight: 48)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("ride-map.save")

                Button(role: .destructive) {
                    isDiscardConfirmationPresented = true
                } label: {
                    Label(localizedAppText("ride_map.discard"), systemImage: "trash")
                        .frame(maxWidth: .infinity, minHeight: 48)
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("ride-map.discard")
            }
        case .draft, .saved, .discarded, .interrupted, .imported, nil:
            startButton
        }
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
