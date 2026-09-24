import CutoutMobile
import Foundation

enum RideMapMetricFormatting {
    static func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }

    static func durationText(for summary: MobileRideMapSummaryDto) -> String {
        Duration.seconds(Double(summary.durationMilliseconds) / 1_000)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
    }

    static func pointCountText(_ count: UInt64) -> String {
        // The app catalog wrapper formats CVarArg values but does not evaluate
        // xcstrings plural substitutions. Keep explicit one/other keys so
        // translators can provide the correct grammar for each locale.
        let key = count == 1 ? "ride_map.point_count.one" : "ride_map.point_count.other"
        return localizedAppText(key, count)
    }

    static func recordedAtText(for milliseconds: UInt64) -> String {
        guard milliseconds > 0 else {
            return localizedAppText("ride_map.untitled_ride")
        }
        return Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
            .formatted(.dateTime.month(.abbreviated).day().year().hour().minute())
    }
}
