#if canImport(ActivityKit) && canImport(WidgetKit) && !os(macOS)
import ActivityKit
import CutoutMobile
import SwiftUI
import WidgetKit

@main
struct CutoutLiveActivityWidgetBundle: WidgetBundle {
    var body: some Widget {
        CutoutRideLiveActivityWidget()
    }
}

private struct CutoutRideLiveActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: LiveActivityRideAttributes.self) { context in
            LockScreenRideActivityView(snapshot: context.state.snapshot)
                .activityBackgroundTint(RideActivityPalette.background)
                .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 5) {
                        BrandMark(size: 15)
                        Text("CUTOUT")
                            .font(.system(size: 12, weight: .bold))
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    HStack(spacing: 5) {
                        Text(context.state.snapshot.identity.displayLabel)
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(RideActivityPalette.secondaryText)
                            .lineLimit(1)
                            .minimumScaleFactor(0.75)
                        Circle()
                            .fill(context.state.snapshot.connectionState == .connected ? RideActivityPalette.connected : RideActivityPalette.warning)
                            .frame(width: 6, height: 6)
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    DynamicIslandRideActivityView(snapshot: context.state.snapshot)
                }
            } compactLeading: {
                HStack(spacing: 4) {
                    BrandMark(size: 18)
                    Text(context.state.snapshot.speed.displayValue)
                        .font(.caption.weight(.bold))
                }
            } compactTrailing: {
                HStack(spacing: 4) {
                    Text(context.state.snapshot.battery.displayValue)
                        .font(.caption.weight(.semibold))
                    Circle()
                        .fill(context.state.snapshot.connectionState == .connected ? RideActivityPalette.connected : RideActivityPalette.warning)
                        .frame(width: 5, height: 5)
                }
            } minimal: {
                BrandMark(size: 16)
            }
        }
    }
}

#else
@main
enum CutoutLiveActivityExtensionFallback {
    static func main() {}
}
#endif
