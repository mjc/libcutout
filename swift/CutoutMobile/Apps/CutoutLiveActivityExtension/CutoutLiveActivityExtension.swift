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
                .activityBackgroundTint(PevLiveActivityPalette.background)
                .activitySystemActionForegroundColor(PevLiveActivityPalette.primaryText)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 5) {
                            PevLiveActivityBrandMark(size: 15)
                            Text("CUTOUT")
                                .font(.caption.weight(.bold))
                        }
                        Text("CUTOUT")
                            .font(.caption.weight(.bold))
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    // The brand mark and wordmark are one visual identity;
                    // expose one concise VoiceOver element instead of
                    // announcing the same name twice.
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("CutOut")
                }
                DynamicIslandExpandedRegion(.trailing) {
                    HStack(spacing: 5) {
                        Text(context.state.snapshot.identity.displayLabel)
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(PevLiveActivityPalette.secondaryText)
                            .lineLimit(1)
                            .minimumScaleFactor(0.75)
                        Circle()
                            .fill(context.state.snapshot.connectionState == .connected ? PevLiveActivityPalette.connected : PevLiveActivityPalette.warning)
                            .frame(width: 6, height: 6)
                            .accessibilityHidden(true)
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("Device")
                    .accessibilityValue(
                        context.state.snapshot.identity.accessibilityValue(
                            for: context.state.snapshot.connectionState
                        )
                    )
                }
                DynamicIslandExpandedRegion(.bottom) {
                    DynamicIslandRideActivityView(snapshot: context.state.snapshot)
                }
            } compactLeading: {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 4) {
                        PevLiveActivityBrandMark(size: 18)
                        Text(context.state.snapshot.speed.displayValue)
                            .font(.caption.weight(.bold))
                    }
                    Text(context.state.snapshot.speed.displayValue)
                        .font(.caption.weight(.bold))
                }
                .frame(maxWidth: .infinity)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(context.state.snapshot.speed.label)
                .accessibilityValue(context.state.snapshot.speed.accessibilityValue)
            } compactTrailing: {
                HStack(spacing: 4) {
                    if context.state.snapshot.headroomSeverity == .reduceAcceleration {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(PevLiveActivityPalette.warning)
                    } else {
                        Text(context.state.snapshot.compactTrailingValue.displayValue)
                            .font(.caption.weight(.semibold))
                    }
                    Circle()
                        .fill(context.state.snapshot.connectionState == .connected ? PevLiveActivityPalette.connected : PevLiveActivityPalette.warning)
                        .frame(width: 5, height: 5)
                        .accessibilityHidden(true)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(context.state.snapshot.compactTrailingValue.label)
                .accessibilityValue(context.state.snapshot.compactTrailingValue.accessibilityValue)
            } minimal: {
                PevLiveActivityBrandMark(size: 16)
                    .accessibilityLabel("CutOut ride")
                    .accessibilityValue(context.state.snapshot.minimalAccessibilitySummary)
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
