#if canImport(ActivityKit) && canImport(WidgetKit) && !os(macOS)
import CutoutMobile
import SwiftUI

struct DynamicIslandRideActivityView: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        ViewThatFits(in: .horizontal) {
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
        }
        .padding(.top, 3)
        .frame(maxWidth: .infinity)
        .foregroundStyle(.white)
        .accessibilityElement(children: .contain)
    }
}

struct LockScreenRideActivityView: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            PevLiveActivityHeader(snapshot: snapshot, compact: false)
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .center, spacing: 12) {
                    VStack(spacing: 5) {
                        PevLiveActivitySpeedGauge(snapshot: snapshot, diameter: 78)
                        PevLiveActivityGlyph(snapshot: snapshot)
                            .frame(width: 54, height: 28)
                    }
                    PevLiveActivityMetricGrid(snapshot: snapshot)
                        .frame(maxWidth: .infinity)
                        .layoutPriority(1)
                }

                VStack(spacing: 7) {
                    PevLiveActivitySpeedGauge(snapshot: snapshot, diameter: 68)
                    PevLiveActivityMetricGrid(snapshot: snapshot, compact: true)
                }
                .frame(maxWidth: .infinity)
            }
            PevLiveActivitySafetyFooter(snapshot: snapshot, compact: false)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity)
        .foregroundStyle(.white)
        .accessibilityElement(children: .contain)
    }
}
#endif
