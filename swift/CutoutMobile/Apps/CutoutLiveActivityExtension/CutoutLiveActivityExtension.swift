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
            LockScreenRideActivityView(snapshot: context.presentationSnapshot)
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
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 5) {
                            Text(context.presentationSnapshot.identity.label)
                                .font(.caption2.weight(.medium))
                                .foregroundStyle(PevLiveActivityPalette.secondaryText)
                            Circle()
                                .fill(context.presentationSnapshot.connectionState == .connected ? PevLiveActivityPalette.connected : PevLiveActivityPalette.warning)
                                .frame(width: 6, height: 6)
                                .accessibilityHidden(true)
                        }
                        .fixedSize(horizontal: true, vertical: false)
                        Circle()
                            .fill(context.presentationSnapshot.connectionState == .connected ? PevLiveActivityPalette.connected : PevLiveActivityPalette.warning)
                            .frame(width: 6, height: 6)
                            .frame(width: 18)
                            .accessibilityHidden(true)
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(LiveActivityRideIdentity.accessibilityLabel)
                    .accessibilityValue(
                        context.presentationSnapshot.identity.accessibilityValue(
                            for: context.presentationSnapshot.connectionState
                        )
                    )
                }
                DynamicIslandExpandedRegion(.bottom) {
                    DynamicIslandRideActivityView(snapshot: context.presentationSnapshot)
                }
            } compactLeading: {
                if context.presentationSnapshot.showsCompactPwmBar {
                    PevLiveActivityCompactPwmBar(snapshot: context.presentationSnapshot)
                } else {
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 4) {
                            PevLiveActivityBrandMark(size: 18)
                            Text(context.presentationSnapshot.speed.displayValue)
                                .font(.caption.weight(.bold))
                        }
                        Text(context.presentationSnapshot.speed.displayValue)
                            .font(.caption.weight(.bold))
                    }
                    .frame(maxWidth: .infinity)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(context.presentationSnapshot.speed.label)
                    .accessibilityValue(context.presentationSnapshot.speed.accessibilityValue)
                }
            } compactTrailing: {
                HStack(spacing: 4) {
                    if context.presentationSnapshot.headroomSeverity == .reduceAcceleration {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(PevLiveActivityPalette.warning)
                    } else {
                        Text(context.presentationSnapshot.compactTrailingValue.displayValue)
                            .font(.caption.weight(.semibold))
                    }
                    Circle()
                        .fill(context.presentationSnapshot.connectionState == .connected ? PevLiveActivityPalette.connected : PevLiveActivityPalette.warning)
                        .frame(width: 5, height: 5)
                        .accessibilityHidden(true)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(context.presentationSnapshot.compactTrailingValue.label)
                .accessibilityValue(context.presentationSnapshot.compactTrailingValue.accessibilityValue)
            } minimal: {
                PevLiveActivityBrandMark(size: 16)
                    .accessibilityLabel(LiveActivityRideSnapshot.activityAccessibilityLabel)
                    .accessibilityValue(context.presentationSnapshot.minimalAccessibilitySummary)
            }
        }
    }
}

private extension ActivityViewContext where Attributes == LiveActivityRideAttributes {
    var presentationSnapshot: LiveActivityRideSnapshot {
        state.snapshot.presented(isStale: isStale)
    }
}

#else
@main
enum CutoutLiveActivityExtensionFallback {
    static func main() {}
}
#endif
