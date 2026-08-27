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
            adaptiveControls {
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
            adaptiveControls {
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
            adaptiveControls {
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
    private func adaptiveControls<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) {
                content()
            }
            VStack(alignment: .leading, spacing: 12) {
                content()
            }
        }
    }

    @ViewBuilder
    private func stopButton(prominent: Bool) -> some View {
        Button(role: .destructive, action: stop) {
            Label(localizedAppText("ride_map.stop"), systemImage: "stop.fill")
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
