#if canImport(ActivityKit) && canImport(WidgetKit) && !os(macOS)
import CutoutMobile
import SwiftUI

struct DynamicIslandRideActivityView: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        ViewThatFits(in: [.horizontal, .vertical]) {
            HStack(alignment: .center, spacing: 8) {
                VStack(spacing: 4) {
                    PevLiveActivitySpeedGauge(snapshot: snapshot, diameter: 70)
                    PevLiveActivityGlyph(snapshot: snapshot)
                        .frame(width: 48, height: 24)
                }
                VStack(spacing: 5) {
                    PevLiveActivityMetricGrid(snapshot: snapshot, compact: true)
                    PevLiveActivitySafetyFooter(snapshot: snapshot, compact: true)
                }
                .frame(maxWidth: .infinity)
                .layoutPriority(1)
            }

            VStack(spacing: 5) {
                HStack(spacing: 10) {
                    PevLiveActivitySpeedGauge(snapshot: snapshot, diameter: 58)
                    PevLiveActivitySafetyFooter(snapshot: snapshot, compact: true)
                        .frame(maxWidth: .infinity)
                }
                PevLiveActivityMetricGrid(snapshot: snapshot, compact: true)
            }
            .frame(maxWidth: .infinity)

            HStack(spacing: 10) {
                PevLiveActivitySpeedGauge(snapshot: snapshot, diameter: 52)
                PevLiveActivitySafetyFooter(snapshot: snapshot, compact: true)
                    .frame(maxWidth: .infinity)
            }
            .frame(maxWidth: .infinity)
        }
        .padding(.top, 3)
        .frame(maxWidth: .infinity)
        .foregroundStyle(PevLiveActivityPalette.primaryText)
        .accessibilityElement(children: .contain)
    }
}

struct LockScreenRideActivityView: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            PevLiveActivityHeader(snapshot: snapshot, compact: true)
            ViewThatFits(in: .vertical) {
                PevLiveActivitySpeedReadout(
                    snapshot: snapshot,
                    speedFontSize: 96,
                    unitFontSize: 16
                )
                PevLiveActivitySpeedReadout(
                    snapshot: snapshot,
                    speedFontSize: 82,
                    unitFontSize: 14
                )
            }
            .frame(maxWidth: .infinity)
            .layoutPriority(1)
            PevLiveActivitySafetyFooter(snapshot: snapshot, compact: false)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity)
        .foregroundStyle(PevLiveActivityPalette.primaryText)
        .accessibilityElement(children: .contain)
    }
}
#endif
