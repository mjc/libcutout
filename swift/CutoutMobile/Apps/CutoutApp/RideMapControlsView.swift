import CutoutMobile
import CutoutMobileFFI
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
        case .recording:
            HStack(spacing: 12) {
                Button(action: pause) {
                    Label(localizedAppText("ride_map.pause"), systemImage: "pause.fill")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("ride-map.pause")
                stopButton(prominent: true)
            }
        case .paused:
            HStack(spacing: 12) {
                Button(action: resume) {
                    Label(localizedAppText("ride_map.resume"), systemImage: "play.fill")
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("ride-map.resume")
                stopButton(prominent: false)
            }
        case .stopped:
            HStack(spacing: 12) {
                Button(action: save) {
                    Label(localizedAppText("ride_map.save"), systemImage: "checkmark.circle.fill")
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("ride-map.save")

                Button(role: .destructive) {
                    isDiscardConfirmationPresented = true
                } label: {
                    Label(localizedAppText("ride_map.discard"), systemImage: "trash")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("ride-map.discard")
            }
        case .saved, .discarded, nil:
            startButton
        }
    }

    @ViewBuilder
    private func stopButton(prominent: Bool) -> some View {
        Button(role: .destructive, action: stop) {
            Label(localizedAppText("ride_map.stop"), systemImage: "stop.fill")
        }
        .modifier(StopButtonStyle(prominent: prominent))
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
            .frame(maxWidth: .infinity, minHeight: 44)
        }
        .buttonStyle(.borderedProminent)
        .accessibilityIdentifier("ride-map.start")
    }
}
