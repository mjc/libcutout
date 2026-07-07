#if canImport(ActivityKit) && canImport(WidgetKit) && !os(macOS)
import CutoutMobile
import SwiftUI

struct DynamicIslandRideActivityView: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            VStack(spacing: 4) {
                PevLiveActivitySpeedGauge(snapshot: snapshot, diameter: 70)
                PevLiveActivityGlyph(snapshot: snapshot)
                    .frame(width: 48, height: 24)
            }
            .frame(width: 78)
            VStack(spacing: 5) {
                PevLiveActivityMetricGrid(snapshot: snapshot, compact: true)
                PevLiveActivitySafetyFooter(snapshot: snapshot, compact: true)
            }
        }
        .padding(.top, 3)
        .foregroundStyle(.white)
    }
}

struct LockScreenRideActivityView: View {
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            PevLiveActivityHeader(snapshot: snapshot, compact: false)
            HStack(alignment: .center, spacing: 12) {
                VStack(spacing: 5) {
                    PevLiveActivitySpeedGauge(snapshot: snapshot, diameter: 78)
                    PevLiveActivityGlyph(snapshot: snapshot)
                        .frame(width: 54, height: 28)
                }
                .frame(width: 88)
                PevLiveActivityMetricGrid(snapshot: snapshot)
            }
            PevLiveActivitySafetyFooter(snapshot: snapshot, compact: false)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .foregroundStyle(.white)
    }
}
#endif
