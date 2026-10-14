import CutoutMobile
import Foundation

enum RideMapMetricFormatting {
    @MainActor
    static func vehicleLabel(
        identity: String?,
        resolve: (String) -> String?,
        noIdentityFallback: String
    ) -> String {
        guard let identity else {
            return noIdentityFallback
        }
        return resolve(identity)
            .flatMap { $0.isEmpty ? nil : $0 }
            ?? localizedAppText("ride_map.vehicle_name_unavailable")
    }

    static func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }

    static func movingTimeText(for summary: MobileRideMapSummaryDto) -> String {
        switch summary.movingTime {
        case .unavailable:
            "--"
        case let .complete(movingMilliseconds, _):
            durationText(milliseconds: movingMilliseconds)
        case let .partial(movingMilliseconds, _, _):
            "≥ \(durationText(milliseconds: movingMilliseconds))"
        }
    }

    static func movingTimeSpokenValue(for summary: MobileRideMapSummaryDto) -> String {
        switch summary.movingTime {
        case .unavailable:
            localizedAppText("ride_map.moving_time_unavailable")
        case let .complete(movingMilliseconds, _):
            durationText(milliseconds: movingMilliseconds)
        case let .partial(movingMilliseconds, _, _):
            localizedAppText("ride_map.moving_time_partial", durationText(milliseconds: movingMilliseconds))
        }
    }

    static func elapsedTimeText(for summary: MobileRideMapSummaryDto) -> String {
        switch summary.elapsedTime {
        case .unavailable:
            "--"
        case let .complete(durationMilliseconds):
            durationText(milliseconds: durationMilliseconds)
        case let .partial(durationMilliseconds):
            "≥ \(durationText(milliseconds: durationMilliseconds))"
        }
    }

    static func elapsedTimeSpokenValue(for summary: MobileRideMapSummaryDto) -> String {
        switch summary.elapsedTime {
        case .unavailable:
            localizedAppText("ride_map.elapsed_time_unavailable")
        case let .complete(durationMilliseconds):
            durationText(milliseconds: durationMilliseconds)
        case let .partial(durationMilliseconds):
            localizedAppText("ride_map.elapsed_time_partial", durationText(milliseconds: durationMilliseconds))
        }
    }

    private static func durationText(milliseconds: UInt64) -> String {
        Duration.seconds(Double(milliseconds) / 1_000)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
    }

    static func recordedAtText(for milliseconds: UInt64) -> String {
        guard milliseconds > 0 else {
            return localizedAppText("ride_map.untitled_ride")
        }
        return Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
            .formatted(.dateTime.month(.abbreviated).day().year().hour().minute())
    }
}
